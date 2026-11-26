//! Online play: one machine hosts (authoritative race, bots, items, hits, laps), friends join by
//! IP or Tailscale name. Each client drives its own kart locally and reports its state; the host
//! sends lobby or race snapshots at 30 Hz. Every message is repeatable state, and race events
//! are acknowledged, so plain unreliable UDP is enough.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::Instant;

use glam::{Quat, Vec3};
use rivet_net::{
    Client, ClientEvent, Host, HostEvent, MAX_PAYLOAD, PeerId, Reader, WireError, Writer,
};
use rivet_physics::{DriftState, VehicleState};

use crate::combat::{CombatEvent, CombatNet, ItemKind};
use crate::race::{Phase, Race, RaceEvent};

/// Identifies Scrapburn and its network protocol version ("NAI" + 1).
pub const GAME_ID: u32 = 0x4e41_4901;
/// Default UDP port (a nod to the arena shooters of old).
pub const DEFAULT_PORT: u16 = 27960;
/// Remote players per host (the host makes eight).
pub const MAX_PEERS: usize = 7;
/// Most racers a snapshot may describe.
const MAX_RACERS: usize = 12;
/// Race events kept for clients that have not acknowledged them yet.
const EVENT_LOG: usize = 96;

const TAG_KART: u8 = 1;
const TAG_LOBBY: u8 = 2;
const TAG_SNAPSHOT: u8 = 3;

// --- Codec ---------------------------------------------------------------------------------

fn put_vec3(w: &mut Writer, v: Vec3) {
    w.f32(v.x).f32(v.y).f32(v.z);
}

fn get_vec3(r: &mut Reader) -> Result<Vec3, WireError> {
    Ok(Vec3::new(r.f32()?, r.f32()?, r.f32()?))
}

fn put_quat(w: &mut Writer, q: Quat) {
    for c in q.normalize().to_array() {
        w.u16(((c.clamp(-1.0, 1.0) * 32767.0).round() as i16) as u16);
    }
}

fn get_quat(r: &mut Reader) -> Result<Quat, WireError> {
    let mut c = [0.0f32; 4];
    for v in &mut c {
        *v = (r.u16()? as i16) as f32 / 32767.0;
    }
    let q = Quat::from_array(c);
    Ok(if q.length_squared() > 0.25 {
        q.normalize()
    } else {
        Quat::IDENTITY
    })
}

pub fn put_state(w: &mut Writer, s: &VehicleState) {
    put_vec3(w, s.position);
    put_quat(w, s.rotation);
    put_vec3(w, s.velocity);
    w.f32(s.angular_yaw);
    let flags = s.grounded as u8
        | (s.drift.is_some() as u8) << 1
        | (s.drift.is_some_and(|d| d.dir > 0.0) as u8) << 2;
    w.u8(flags);
    if let Some(d) = s.drift {
        w.f32(d.charge);
    }
    w.f32(s.boost_timer).f32(s.boost_speed);
    w.u8((s.offroad.clamp(0.0, 1.0) * 255.0) as u8);
    w.f32(s.spin);
}

pub fn get_state(r: &mut Reader) -> Result<VehicleState, WireError> {
    let position = get_vec3(r)?;
    let rotation = get_quat(r)?;
    let velocity = get_vec3(r)?;
    let angular_yaw = r.f32()?;
    let flags = r.u8()?;
    let drift = if flags & 2 != 0 {
        Some(DriftState {
            dir: if flags & 4 != 0 { 1.0 } else { -1.0 },
            charge: r.f32()?.clamp(0.0, 30.0),
        })
    } else {
        None
    };
    let (boost_timer, boost_speed) = (r.f32()?.clamp(0.0, 10.0), r.f32()?.clamp(0.0, 40.0));
    let offroad = r.u8()? as f32 / 255.0;
    let spin = r.f32()?.clamp(0.0, 10.0);
    // Positions and speeds far outside any track are garbage, not physics.
    if position.abs().max_element() > 1.0e5 || velocity.length() > 500.0 {
        return Err(WireError::Invalid("kart state out of range"));
    }
    Ok(VehicleState {
        position,
        rotation,
        velocity,
        angular_yaw: angular_yaw.clamp(-50.0, 50.0),
        grounded: flags & 1 != 0,
        drift,
        boost_timer,
        boost_speed,
        offroad,
        spin,
        // Spin bookkeeping stays on the kart's own machine.
        spin_from: 0.0,
        spin_total: spin,
    })
}

fn item_code(i: Option<ItemKind>) -> u8 {
    match i {
        None => 0,
        Some(ItemKind::Nailer) => 1,
        Some(ItemKind::Harpoon) => 2,
        Some(ItemKind::PlasmaOrb) => 3,
    }
}

fn item_from(c: u8) -> Result<Option<ItemKind>, WireError> {
    Ok(match c {
        0 => None,
        1 => Some(ItemKind::Nailer),
        2 => Some(ItemKind::Harpoon),
        3 => Some(ItemKind::PlasmaOrb),
        _ => return Err(WireError::Invalid("item")),
    })
}

fn some_item(c: u8) -> Result<ItemKind, WireError> {
    item_from(c)?.ok_or(WireError::Invalid("item"))
}

fn put_event(w: &mut Writer, e: &RaceEvent) {
    match *e {
        RaceEvent::Go => {
            w.u8(0);
        }
        RaceEvent::Lap {
            racer,
            number,
            time,
        } => {
            w.u8(1)
                .u8(racer as u8)
                .u16(number.min(u16::MAX as u32) as u16)
                .f64(time);
        }
        RaceEvent::Finish { racer, place } => {
            w.u8(2).u8(racer as u8).u8(place as u8);
        }
        RaceEvent::Eliminated { racer, place } => {
            w.u8(3).u8(racer as u8).u8(place as u8);
        }
        RaceEvent::Combat(c) => match c {
            CombatEvent::Pickup { racer, item } => {
                w.u8(10).u8(racer as u8).u8(item_code(Some(item)));
            }
            CombatEvent::Fire { racer, item } => {
                w.u8(11).u8(racer as u8).u8(item_code(Some(item)));
            }
            CombatEvent::Hit { victim, item } => {
                w.u8(12).u8(victim as u8).u8(item_code(Some(item)));
            }
            CombatEvent::Slingshot { racer } => {
                w.u8(13).u8(racer as u8);
            }
            CombatEvent::Zap { victim } => {
                w.u8(14).u8(victim as u8);
            }
            CombatEvent::Explode { victim, pos } => {
                w.u8(15).u8(victim as u8);
                put_vec3(w, pos);
            }
        },
    }
}

/// Largest encoded event (Lap: 1 + 1 + 2 + 8 bytes; Explode: 1 + 1 + 12).
const MAX_EVENT_BYTES: usize = 14;

fn get_event(r: &mut Reader, racers: usize) -> Result<RaceEvent, WireError> {
    let tag = r.u8()?;
    let idx = |r: &mut Reader| -> Result<usize, WireError> {
        let i = r.u8()? as usize;
        if i < racers {
            Ok(i)
        } else {
            Err(WireError::Invalid("racer index"))
        }
    };
    Ok(match tag {
        0 => RaceEvent::Go,
        1 => RaceEvent::Lap {
            racer: idx(r)?,
            number: r.u16()? as u32,
            time: r.f64()?,
        },
        2 => RaceEvent::Finish {
            racer: idx(r)?,
            place: r.u8()? as u32,
        },
        3 => RaceEvent::Eliminated {
            racer: idx(r)?,
            place: r.u8()? as u32,
        },
        10 => RaceEvent::Combat(CombatEvent::Pickup {
            racer: idx(r)?,
            item: some_item(r.u8()?)?,
        }),
        11 => RaceEvent::Combat(CombatEvent::Fire {
            racer: idx(r)?,
            item: some_item(r.u8()?)?,
        }),
        12 => RaceEvent::Combat(CombatEvent::Hit {
            victim: idx(r)?,
            item: some_item(r.u8()?)?,
        }),
        13 => RaceEvent::Combat(CombatEvent::Slingshot { racer: idx(r)? }),
        14 => RaceEvent::Combat(CombatEvent::Zap { victim: idx(r)? }),
        15 => RaceEvent::Combat(CombatEvent::Explode {
            victim: idx(r)?,
            pos: get_vec3(r)?,
        }),
        _ => return Err(WireError::Invalid("event")),
    })
}

/// Client → host: this machine's kart.
#[derive(Debug, Clone, PartialEq)]
pub struct KartMsg {
    pub state: VehicleState,
    /// Total fire presses so far (the host fires once per increment).
    pub fires: u32,
    /// Newest race event id this client has applied.
    pub ack: u32,
    /// Race this state belongs to (stale states from a previous race are ignored).
    pub race_id: u32,
}

impl KartMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(TAG_KART)
            .u32(self.race_id)
            .u32(self.fires)
            .u32(self.ack);
        put_state(&mut w, &self.state);
        w.finish()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(bytes);
        if r.u8()? != TAG_KART {
            return Err(WireError::Invalid("tag"));
        }
        Ok(Self {
            race_id: r.u32()?,
            fires: r.u32()?,
            ack: r.u32()?,
            state: get_state(&mut r)?,
        })
    }
}

/// What the host has set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaceSetup {
    pub track: u8,
    pub elimination: bool,
    pub laps: u8,
    pub rivals: u8,
}

/// Host → client while waiting in the lobby.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lobby {
    pub setup: RaceSetup,
    /// Host first, then joined players.
    pub names: Vec<String>,
    /// This client's slot in `names`.
    pub you: u8,
}

fn put_setup(w: &mut Writer, s: &RaceSetup) {
    w.u8(s.track).bool(s.elimination).u8(s.laps).u8(s.rivals);
}

fn get_setup(r: &mut Reader) -> Result<RaceSetup, WireError> {
    let s = RaceSetup {
        track: r.u8()?,
        elimination: r.bool()?,
        laps: r.u8()?,
        rivals: r.u8()?,
    };
    if s.track as usize >= crate::track::TRACKS.len()
        || s.laps == 0
        || s.rivals as usize >= MAX_RACERS
    {
        return Err(WireError::Invalid("race setup"));
    }
    Ok(s)
}

fn put_names(w: &mut Writer, names: &[String]) {
    w.u8(names.len() as u8);
    for n in names {
        w.str(n);
    }
}

fn get_names(r: &mut Reader) -> Result<Vec<String>, WireError> {
    let n = r.u8()? as usize;
    if n > MAX_RACERS {
        return Err(WireError::Invalid("name count"));
    }
    (0..n).map(|_| r.str().map(rivet_net::clean_name)).collect()
}

impl Lobby {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(TAG_LOBBY);
        put_setup(&mut w, &self.setup);
        w.u8(self.you);
        put_names(&mut w, &self.names);
        w.finish()
    }

    fn decode_body(r: &mut Reader) -> Result<Self, WireError> {
        let setup = get_setup(r)?;
        let you = r.u8()?;
        let names = get_names(r)?;
        Ok(Self { setup, names, you })
    }
}

/// One racer as seen in a snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct RacerNet {
    pub state: VehicleState,
    pub next: u16,
    pub laps: u16,
    pub lap_start: f64,
    pub finish: Option<(u32, f64)>,
    pub out: bool,
    pub hits: u8,
    pub last_hit: (f32, f32),
    pub last_lap: Option<f64>,
    pub best_lap: Option<f64>,
}

/// Host → client during a race.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub race_id: u32,
    pub setup: RaceSetup,
    pub you: u8,
    pub names: Vec<String>,
    /// Countdown seconds left, or 0 once racing.
    pub countdown: f32,
    pub time: f64,
    pub racers: Vec<RacerNet>,
    pub combat: CombatNet,
    /// Id of `events[0]`; the rest follow consecutively.
    pub first_event: u32,
    pub events: Vec<RaceEvent>,
}

fn put_opt_f64(w: &mut Writer, v: Option<f64>) {
    match v {
        Some(t) => {
            w.bool(true).f32(t as f32);
        }
        None => {
            w.bool(false);
        }
    }
}

fn get_opt_f64(r: &mut Reader) -> Result<Option<f64>, WireError> {
    Ok(if r.bool()? {
        Some(r.f32()? as f64)
    } else {
        None
    })
}

impl Snapshot {
    /// Encodes as much as fits in one datagram: racers always, then combat objects and events
    /// as space allows (events left out are resent, since clients acknowledge them).
    pub fn encode(&self) -> Vec<u8> {
        let bytes = self.encode_with(true);
        if bytes.len() <= MAX_PAYLOAD {
            bytes
        } else {
            // Never send an oversized datagram: drop the extras (events are resent later).
            self.encode_with(false)
        }
    }

    fn encode_with(&self, extras: bool) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(TAG_SNAPSHOT).u32(self.race_id);
        put_setup(&mut w, &self.setup);
        w.u8(self.you);
        put_names(&mut w, &self.names);
        w.f32(self.countdown).f64(self.time);
        w.u8(self.racers.len() as u8);
        for r in &self.racers {
            put_state(&mut w, &r.state);
            w.u16(r.next).u16(r.laps).f64(r.lap_start);
            match r.finish {
                Some((place, t)) => {
                    w.u8(place as u8).f32(t as f32);
                }
                None => {
                    w.u8(0);
                }
            }
            w.bool(r.out).u8(r.hits).f32(r.last_hit.0).f32(r.last_hit.1);
            put_opt_f64(&mut w, r.last_lap);
            put_opt_f64(&mut w, r.best_lap);
        }
        let c = &self.combat;
        w.u8(c.boxes.len().min(64) as u8);
        let mut bits = 0u64;
        for (i, &b) in c.boxes.iter().take(64).enumerate() {
            bits |= (b as u64) << i;
        }
        w.u32(bits as u32).u32((bits >> 32) as u32);
        w.u8(c.held.len().min(MAX_RACERS) as u8);
        for &h in c.held.iter().take(MAX_RACERS) {
            w.u8(item_code(h));
        }
        let orbs = if extras { c.orbs.len().min(4) } else { 0 };
        let hooks = if extras { c.hooks.len().min(8) } else { 0 };
        // Room left after orbs, hooks, the counts and at least a few events.
        let fixed = orbs * 13 + hooks * 15 + 3 + 4 + 1 + 4 * MAX_EVENT_BYTES;
        let room = MAX_PAYLOAD.saturating_sub(w.len() + fixed);
        let nails = if extras {
            c.nails.len().min(room / 24).min(255)
        } else {
            0
        };
        w.u8(nails as u8);
        for &(p, v) in c.nails.iter().take(nails) {
            put_vec3(&mut w, p);
            put_vec3(&mut w, v);
        }
        w.u8(orbs as u8);
        for &(p, t) in c.orbs.iter().take(orbs) {
            put_vec3(&mut w, p);
            w.u8(t as u8);
        }
        w.u8(hooks as u8);
        for &(o, p, t) in c.hooks.iter().take(hooks) {
            w.u8(o as u8);
            put_vec3(&mut w, p);
            w.u8(t.map_or(255, |t| t as u8));
            w.u8(0);
        }
        let room = MAX_PAYLOAD.saturating_sub(w.len() + 5);
        let events = if extras {
            self.events.len().min(room / MAX_EVENT_BYTES).min(255)
        } else {
            0
        };
        w.u32(self.first_event).u8(events as u8);
        for e in self.events.iter().take(events) {
            put_event(&mut w, e);
        }
        w.finish()
    }

    fn decode_body(r: &mut Reader) -> Result<Self, WireError> {
        let race_id = r.u32()?;
        let setup = get_setup(r)?;
        let you = r.u8()?;
        let names = get_names(r)?;
        let countdown = r.f32()?.clamp(0.0, 30.0);
        let time = r.f64()?;
        let n = r.u8()? as usize;
        if n == 0 || n > MAX_RACERS || you as usize >= n {
            return Err(WireError::Invalid("racer count"));
        }
        let mut racers = Vec::with_capacity(n);
        for _ in 0..n {
            let state = get_state(r)?;
            let (next, laps, lap_start) = (r.u16()?, r.u16()?, r.f64()?);
            let place = r.u8()?;
            let finish = if place > 0 {
                Some((place as u32, r.f32()? as f64))
            } else {
                None
            };
            racers.push(RacerNet {
                state,
                next,
                laps,
                lap_start,
                finish,
                out: r.bool()?,
                hits: r.u8()?,
                last_hit: (r.f32()?.clamp(0.0, 10.0), r.f32()?.clamp(0.0, 1.0)),
                last_lap: get_opt_f64(r)?,
                best_lap: get_opt_f64(r)?,
            });
        }
        let nb = r.u8()? as usize;
        let bits = r.u32()? as u64 | (r.u32()? as u64) << 32;
        let boxes = (0..nb.min(64)).map(|i| bits >> i & 1 == 1).collect();
        let nh = r.u8()? as usize;
        if nh > MAX_RACERS {
            return Err(WireError::Invalid("held count"));
        }
        let held = (0..nh)
            .map(|_| item_from(r.u8()?))
            .collect::<Result<_, _>>()?;
        let nn = r.u8()? as usize;
        let nails = (0..nn)
            .map(|_| Ok((get_vec3(r)?, get_vec3(r)?)))
            .collect::<Result<_, WireError>>()?;
        let no = r.u8()? as usize;
        let mut orbs = Vec::new();
        for _ in 0..no {
            let p = get_vec3(r)?;
            let t = r.u8()? as usize;
            if t < n {
                orbs.push((p, t));
            }
        }
        let nk = r.u8()? as usize;
        let mut hooks = Vec::new();
        for _ in 0..nk {
            let o = r.u8()? as usize;
            let p = get_vec3(r)?;
            let t = r.u8()?;
            let _reserved = r.u8()?;
            let t = (t != 255).then_some(t as usize);
            if o < n && t.is_none_or(|t| t < n) {
                hooks.push((o, p, t));
            }
        }
        let first_event = r.u32()?;
        let ne = r.u8()? as usize;
        let events = (0..ne).map(|_| get_event(r, n)).collect::<Result<_, _>>()?;
        Ok(Self {
            race_id,
            setup,
            you,
            names,
            countdown,
            time,
            racers,
            combat: CombatNet {
                boxes,
                held,
                nails,
                orbs,
                hooks,
            },
            first_event,
            events,
        })
    }
}

/// Anything the host sends.
#[derive(Debug, Clone, PartialEq)]
pub enum ToClient {
    Lobby(Lobby),
    Snapshot(Box<Snapshot>),
}

impl ToClient {
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(bytes);
        match r.u8()? {
            TAG_LOBBY => Ok(ToClient::Lobby(Lobby::decode_body(&mut r)?)),
            TAG_SNAPSHOT => Ok(ToClient::Snapshot(Box::new(Snapshot::decode_body(&mut r)?))),
            _ => Err(WireError::Invalid("tag")),
        }
    }
}

// --- Host session ----------------------------------------------------------------------------

/// Lobby and race changes the menus show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostNote {
    Joined(String),
    Left(String),
}

/// The hosting side of an online game.
pub struct NetHost {
    link: Host,
    pub setup: RaceSetup,
    pub host_name: String,
    /// Race seat of each peer slot while racing.
    seats: Vec<Option<usize>>,
    race_id: u32,
    in_race: bool,
    events: VecDeque<(u32, RaceEvent)>,
    next_event: u32,
    acks: Vec<u32>,
    fires: Vec<u32>,
}

impl NetHost {
    pub fn bind(port: u16, setup: RaceSetup, host_name: &str) -> std::io::Result<Self> {
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        Ok(Self {
            link: Host::bind(addr, MAX_PEERS, GAME_ID)?,
            setup,
            host_name: rivet_net::clean_name(host_name),
            seats: vec![None; MAX_PEERS],
            race_id: 0,
            in_race: false,
            events: VecDeque::new(),
            next_event: 1,
            acks: vec![0; MAX_PEERS],
            fires: vec![0; MAX_PEERS],
        })
    }

    pub fn port(&self) -> u16 {
        self.link.local_addr().map_or(0, |a| a.port())
    }

    /// Host first, then connected players.
    pub fn names(&self) -> Vec<String> {
        std::iter::once(self.host_name.clone())
            .chain(self.link.peers().map(|(_, n)| n.to_string()))
            .collect()
    }

    pub fn player_count(&self) -> usize {
        1 + self.link.peers().count()
    }

    pub fn in_race(&self) -> bool {
        self.in_race
    }

    /// Racers for a race with `rivals` set in the menu: everyone connected, bots fill the rest.
    pub fn racer_count(&self) -> usize {
        (self.setup.rivals as usize + 1).max(self.player_count())
    }

    /// Seats connected players in `race` (the host is seat 0) and starts sending snapshots.
    pub fn start(&mut self, race: &mut Race) {
        self.race_id = self.race_id.wrapping_add(1);
        self.in_race = true;
        self.events.clear();
        self.seats = vec![None; MAX_PEERS];
        self.acks = vec![self.next_event.wrapping_sub(1); MAX_PEERS];
        let peers: Vec<(PeerId, String)> =
            self.link.peers().map(|(p, n)| (p, n.to_string())).collect();
        race.racers[0].name = self.host_name.clone().into();
        for (k, (peer, name)) in peers.into_iter().enumerate() {
            let seat = k + 1;
            if seat >= race.racers.len() {
                break;
            }
            race.set_remote(seat);
            race.racers[seat].name = name.into();
            self.seats[peer.0 as usize] = Some(seat);
            self.fires[peer.0 as usize] = 0;
        }
    }

    /// Back to the lobby.
    pub fn end_race(&mut self) {
        self.in_race = false;
    }

    /// Network input for this tick: remote kart states and fire presses go into `race`.
    pub fn poll(&mut self, now: Instant, mut race: Option<&mut Race>) -> Vec<HostNote> {
        let mut notes = Vec::new();
        for e in self.link.poll(now) {
            match e {
                HostEvent::Joined { name, .. } => notes.push(HostNote::Joined(name)),
                HostEvent::Left { peer } => {
                    let slot = peer.0 as usize;
                    notes.push(HostNote::Left(format!("PLAYER {}", slot + 2)));
                    if let (Some(seat), Some(race)) = (self.seats[slot].take(), race.as_deref_mut())
                    {
                        // The kart carries on as a bot.
                        race.make_bot(seat);
                    }
                }
                HostEvent::Message { peer, data } => {
                    let slot = peer.0 as usize;
                    let Ok(msg) = KartMsg::decode(&data) else {
                        continue;
                    };
                    if !self.in_race || msg.race_id != self.race_id {
                        continue;
                    }
                    let (Some(seat), Some(race)) = (self.seats[slot], race.as_deref_mut()) else {
                        continue;
                    };
                    race.push_remote_state(seat, msg.state);
                    // One shot per press, at most a few per message.
                    let presses = msg.fires.wrapping_sub(self.fires[slot]).min(3);
                    if presses > 0 && msg.fires > self.fires[slot] {
                        race.fire(seat);
                    }
                    self.fires[slot] = self.fires[slot].max(msg.fires);
                    if msg.ack.wrapping_sub(self.acks[slot]) < 1 << 30 {
                        self.acks[slot] = msg.ack;
                    }
                }
            }
        }
        notes
    }

    /// Logs this tick's race events for the clients.
    pub fn record(&mut self, events: &[RaceEvent]) {
        for e in events {
            self.events.push_back((self.next_event, *e));
            self.next_event = self.next_event.wrapping_add(1);
            if self.events.len() > EVENT_LOG {
                self.events.pop_front();
            }
        }
    }

    /// Sends every peer the lobby or a race snapshot.
    pub fn send(&mut self, race: Option<&Race>) {
        let names = self.names();
        let peers: Vec<PeerId> = self.link.peers().map(|(p, _)| p).collect();
        for peer in peers {
            let slot = peer.0 as usize;
            let bytes = match (self.in_race, race, self.seats[slot]) {
                (true, Some(race), Some(seat)) => {
                    let pending: Vec<&(u32, RaceEvent)> = self
                        .events
                        .iter()
                        .filter(|(id, _)| {
                            id.wrapping_sub(self.acks[slot]).wrapping_sub(1) < 1 << 30
                        })
                        .collect();
                    let first_event = pending.first().map_or(self.next_event, |e| e.0);
                    let snap = snapshot(
                        race,
                        self.race_id,
                        self.setup,
                        seat as u8,
                        first_event,
                        pending.iter().map(|e| e.1).collect(),
                    );
                    snap.encode()
                }
                // In the lobby, or joined after the race began: show the lobby.
                _ => Lobby {
                    setup: self.setup,
                    names: names.clone(),
                    you: (slot + 1) as u8,
                }
                .encode(),
            };
            let _ = self.link.send(peer, &bytes);
        }
    }
}

/// A snapshot of `race` for the client in seat `you`.
pub fn snapshot(
    race: &Race,
    race_id: u32,
    setup: RaceSetup,
    you: u8,
    first_event: u32,
    events: Vec<RaceEvent>,
) -> Snapshot {
    Snapshot {
        race_id,
        setup,
        you,
        names: race.racers.iter().map(|r| r.name.to_string()).collect(),
        countdown: match race.phase {
            Phase::Countdown(left) => left.max(0.001),
            Phase::Racing => 0.0,
        },
        time: race.time,
        racers: race
            .racers
            .iter()
            .map(|r| {
                let (next, laps, lap_start) = r.laps.net_state();
                RacerNet {
                    state: r.state.clone(),
                    next,
                    laps,
                    lap_start,
                    finish: r.finish,
                    out: r.out,
                    hits: r.hits as u8,
                    last_hit: r.last_hit,
                    last_lap: r.last_lap,
                    best_lap: r.best_lap,
                }
            })
            .collect(),
        combat: race.combat.net(),
        first_event,
        events,
    }
}

// --- Client view -----------------------------------------------------------------------------

/// Client-side bookkeeping for showing a host's race smoothly.
#[derive(Debug, Clone)]
pub struct ClientView {
    pub you: usize,
    pub race_id: u32,
    /// Newest event applied.
    pub last_event: u32,
    /// Last snapshot position per kart, extrapolated each tick.
    base: Vec<Vec3>,
    /// Display offset per kart, decaying toward the snapshot.
    err: Vec<Vec3>,
    /// Ticks since the last snapshot.
    stale: u32,
    seen_hits: Option<u8>,
}

impl ClientView {
    pub fn new(you: usize, race_id: u32, racers: usize) -> Self {
        Self {
            you,
            race_id,
            last_event: 0,
            base: vec![Vec3::ZERO; racers],
            err: vec![Vec3::ZERO; racers],
            stale: 0,
            seen_hits: None,
        }
    }

    /// Applies a snapshot to the client's copy of the race. Returns the race events not seen
    /// before, oldest first.
    pub fn apply(&mut self, race: &mut Race, snap: &Snapshot) -> Vec<RaceEvent> {
        let n = race.racers.len().min(snap.racers.len());
        race.time = snap.time;
        race.phase = if snap.countdown > 0.0 {
            Phase::Countdown(snap.countdown)
        } else {
            Phase::Racing
        };
        for i in 0..n {
            let s = &snap.racers[i];
            let r = &mut race.racers[i];
            if let Some(name) = snap.names.get(i)
                && r.name != name.as_str()
            {
                r.name = name.clone().into();
            }
            r.laps.set_net_state(s.next, s.laps, s.lap_start);
            r.finish = s.finish;
            r.out = s.out;
            r.last_lap = s.last_lap;
            r.best_lap = s.best_lap;
            if i == self.you {
                // Weapon hits the host saw land on this kart.
                match self.seen_hits {
                    Some(seen) if seen != s.hits => {
                        r.state.spin_out(s.last_hit.0, s.last_hit.1);
                    }
                    _ => {}
                }
                self.seen_hits = Some(s.hits);
                // Finished: the autopilot takes the cool-down lap.
                if r.finish.is_some() && r.bot.is_none() && !r.out {
                    r.bot = Some(rivet_ai::BotDriver::new(0.85, 0.0));
                }
                continue;
            }
            let shown = r.state.position;
            let fresh = self.base[i] == Vec3::ZERO;
            r.state = s.state.clone();
            self.base[i] = s.state.position;
            let gap = shown - s.state.position;
            self.err[i] = if fresh || gap.length() > 8.0 {
                Vec3::ZERO
            } else {
                gap
            };
            r.state.position = self.base[i] + self.err[i];
            if fresh {
                r.prev = r.state.clone();
            }
        }
        race.combat.apply_net(&snap.combat);
        self.stale = 0;
        let mut out = Vec::new();
        for (k, e) in snap.events.iter().enumerate() {
            let id = snap.first_event.wrapping_add(k as u32);
            if id.wrapping_sub(self.last_event).wrapping_sub(1) < 1 << 30 {
                out.push(*e);
                self.last_event = id;
            }
        }
        out
    }

    /// Advances the other karts between snapshots (extrapolation for a quarter second at most,
    /// with the correction offset melting away).
    pub fn tick_others(&mut self, race: &mut Race, dt: f32) {
        self.stale += 1;
        let extrapolate = self.stale < 15;
        let decay = (-12.0 * dt).exp();
        for (i, r) in race.racers.iter_mut().enumerate() {
            if i == self.you || i >= self.base.len() {
                continue;
            }
            r.prev = r.state.clone();
            if extrapolate && !r.out {
                self.base[i] += r.state.velocity * dt;
            }
            self.err[i] *= decay;
            r.state.position = self.base[i] + self.err[i];
            if race.phase == Phase::Racing {
                r.state.spin = (r.state.spin - dt).max(0.0);
            }
        }
        if race.phase == Phase::Racing {
            race.time += dt as f64;
        }
    }
}

/// What the client's menus and game need to react to.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientNote {
    Connected,
    Rejected(String),
    Lost,
    Lobby(Lobby),
    Snapshot(Box<Snapshot>),
}

/// The joining side of an online game.
pub struct NetClient {
    link: Client,
    pub fires: u32,
}

impl NetClient {
    pub fn connect(server: SocketAddr, name: &str) -> std::io::Result<Self> {
        Ok(Self {
            link: Client::connect(server, name, GAME_ID, Instant::now())?,
            fires: 0,
        })
    }

    pub fn server(&self) -> SocketAddr {
        self.link.server()
    }

    pub fn connected(&self) -> bool {
        self.link.slot().is_some()
    }

    pub fn poll(&mut self, now: Instant) -> Vec<ClientNote> {
        let mut notes = Vec::new();
        for e in self.link.poll(now) {
            match e {
                ClientEvent::Connected { .. } => notes.push(ClientNote::Connected),
                ClientEvent::Rejected { reason } => notes.push(ClientNote::Rejected(reason)),
                ClientEvent::Lost => notes.push(ClientNote::Lost),
                ClientEvent::Message { data } => match ToClient::decode(&data) {
                    Ok(ToClient::Lobby(l)) => notes.push(ClientNote::Lobby(l)),
                    Ok(ToClient::Snapshot(s)) => notes.push(ClientNote::Snapshot(s)),
                    Err(e) => log::debug!("dropped a bad packet from the host: {e}"),
                },
            }
        }
        notes
    }

    /// Reports this machine's kart.
    pub fn send_kart(&mut self, state: &VehicleState, view: &ClientView, now: Instant) {
        let msg = KartMsg {
            state: state.clone(),
            fires: self.fires,
            ack: view.last_event,
            race_id: view.race_id,
        };
        let _ = self.link.send(&msg.encode(), now);
    }
}

/// Resolves "host", "host:port", "1.2.3.4" or "[::1]:port" (default port added).
pub fn resolve(addr: &str) -> Option<SocketAddr> {
    use std::net::ToSocketAddrs;
    let addr = addr.trim();
    if addr.is_empty() {
        return None;
    }
    if let Ok(a) = addr.parse::<SocketAddr>() {
        return Some(a);
    }
    if let Ok(ip) = addr.parse::<std::net::IpAddr>() {
        return Some(SocketAddr::new(ip, DEFAULT_PORT));
    }
    let with_port = if addr.contains(':') {
        addr.to_string()
    } else {
        format!("{addr}:{DEFAULT_PORT}")
    };
    // The host listens on IPv4; prefer an IPv4 address when a name has both.
    let addrs: Vec<SocketAddr> = with_port.to_socket_addrs().ok()?.collect();
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or(addrs.first())
        .copied()
}

// --- Tailscale ---------------------------------------------------------------------------------

/// An online machine on this tailnet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailnetPeer {
    pub name: String,
    pub ip: std::net::IpAddr,
}

/// Online peers from `tailscale status --json` output.
pub fn parse_tailscale_status(json: &str) -> Vec<TailnetPeer> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(peers) = v.get("Peer").and_then(|p| p.as_object()) else {
        return Vec::new();
    };
    let mut out: Vec<TailnetPeer> = peers
        .values()
        .filter(|p| p.get("Online").and_then(|o| o.as_bool()).unwrap_or(false))
        .filter_map(|p| {
            let name = p.get("HostName")?.as_str()?;
            let ip = p
                .get("TailscaleIPs")?
                .as_array()?
                .iter()
                .filter_map(|i| i.as_str()?.parse::<std::net::IpAddr>().ok())
                .find(|ip| ip.is_ipv4())?;
            Some(TailnetPeer {
                name: rivet_net::clean_name(name).to_ascii_uppercase(),
                ip,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Asks the local Tailscale client for online peers (empty if Tailscale isn't installed).
pub fn tailscale_peers() -> Vec<TailnetPeer> {
    std::process::Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_tailscale_status(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::build_graybox_loop;
    use rivet_physics::DriveInput;

    fn setup() -> RaceSetup {
        RaceSetup {
            track: 0,
            elimination: false,
            laps: 3,
            rivals: 3,
        }
    }

    #[test]
    fn kart_message_round_trips() {
        let mut s = VehicleState::spawn(Vec3::new(1.0, 2.0, 3.0), 0.7);
        s.velocity = Vec3::new(10.0, 0.0, -4.0);
        s.drift = Some(DriftState {
            dir: 1.0,
            charge: 0.8,
        });
        s.boost_timer = 0.5;
        s.spin = 0.2;
        let m = KartMsg {
            state: s.clone(),
            fires: 4,
            ack: 9,
            race_id: 2,
        };
        let back = KartMsg::decode(&m.encode()).unwrap();
        assert_eq!((back.fires, back.ack, back.race_id), (4, 9, 2));
        assert_eq!(back.state.position, s.position);
        assert!(back.state.rotation.angle_between(s.rotation) < 1e-3);
        assert_eq!(back.state.drift, s.drift);
        assert_eq!(back.state.spin, s.spin);
    }

    #[test]
    fn garbage_is_rejected_not_trusted() {
        assert!(KartMsg::decode(&[TAG_KART, 1, 2]).is_err());
        assert!(ToClient::decode(&[]).is_err());
        assert!(ToClient::decode(&[99]).is_err());
        // A kart teleported to the edge of the universe.
        let mut s = VehicleState::spawn(Vec3::ZERO, 0.0);
        s.position.x = 1.0e9;
        let m = KartMsg {
            state: s,
            fires: 0,
            ack: 0,
            race_id: 0,
        };
        assert!(KartMsg::decode(&m.encode()).is_err());
        // A lobby naming a track that doesn't exist.
        let mut l = Lobby {
            setup: setup(),
            names: vec!["A".into()],
            you: 1,
        };
        l.setup.track = 200;
        assert!(ToClient::decode(&l.encode()).is_err());
    }

    #[test]
    fn snapshot_round_trips_and_fits_one_datagram() {
        let t = build_graybox_loop();
        let mut race = Race::new(&t, 8, true, 3);
        for _ in 0..(6 * 60) {
            race.step(
                &t,
                DriveInput {
                    throttle: 1.0,
                    ..Default::default()
                },
                1.0 / 60.0,
            );
        }
        // A busy moment: lots of nails in the air and a pile of events.
        let mut combat = race.combat.net();
        combat.nails = vec![(Vec3::ONE, Vec3::X); 60];
        let snap = Snapshot {
            combat,
            ..snapshot(&race, 7, setup(), 2, 40, vec![RaceEvent::Go; 120])
        };
        let bytes = snap.encode();
        assert!(bytes.len() <= MAX_PAYLOAD, "{} bytes", bytes.len());
        let ToClient::Snapshot(back) = ToClient::decode(&bytes).unwrap() else {
            panic!("not a snapshot")
        };
        assert_eq!(back.race_id, 7);
        assert_eq!(back.racers.len(), 8);
        assert_eq!(back.names, snap.names);
        for (a, b) in back.racers.iter().zip(&snap.racers) {
            assert_eq!(a.state.position, b.state.position);
            assert_eq!((a.next, a.laps), (b.next, b.laps));
        }
        assert!(
            !back.events.is_empty() && back.events.len() < 120,
            "events trimmed to fit"
        );
        assert!(!back.combat.nails.is_empty());
    }

    #[test]
    fn client_view_applies_events_once_and_hits_to_its_own_kart() {
        let t = build_graybox_loop();
        let host = Race::new(&t, 3, true, 3);
        let mut client = Race::new(&t, 3, true, 3);
        let mut view = ClientView::new(1, 1, 3);
        let mut snap = snapshot(&host, 1, setup(), 1, 5, vec![RaceEvent::Go, RaceEvent::Go]);
        assert_eq!(view.apply(&mut client, &snap).len(), 2);
        assert!(view.apply(&mut client, &snap).is_empty(), "no repeats");
        snap.first_event = 6;
        snap.events = vec![
            RaceEvent::Go,
            RaceEvent::Lap {
                racer: 0,
                number: 1,
                time: 3.0,
            },
        ];
        assert_eq!(view.apply(&mut client, &snap).len(), 1, "only the new one");

        client.racers[1].state.velocity = Vec3::new(0.0, 0.0, 20.0);
        snap.racers[1].hits = snap.racers[1].hits.wrapping_add(1);
        snap.racers[1].last_hit = (1.1, 0.5);
        view.apply(&mut client, &snap);
        assert!(
            client.racers[1].state.spin > 1.0,
            "the host's hit spins our kart"
        );
        assert_eq!(client.racers[0].state.spin, 0.0);
    }

    #[test]
    fn other_karts_glide_toward_corrections() {
        let t = build_graybox_loop();
        let host = Race::new(&t, 2, true, 3);
        let mut client = Race::new(&t, 2, true, 3);
        let mut view = ClientView::new(0, 1, 2);
        let mut snap = snapshot(&host, 1, setup(), 0, 1, Vec::new());
        view.apply(&mut client, &snap);
        let p0 = client.racers[1].state.position;
        snap.racers[1].state.position += Vec3::new(2.0, 0.0, 0.0);
        view.apply(&mut client, &snap);
        assert_eq!(client.racers[1].state.position, p0, "no visible jump");
        for _ in 0..30 {
            view.tick_others(&mut client, 1.0 / 60.0);
        }
        let target = snap.racers[1].state.position;
        assert!(client.racers[1].state.position.distance(target) < 0.05);
    }

    #[test]
    fn host_and_client_play_over_loopback() {
        let t = build_graybox_loop();
        let mut host = NetHost::bind(0, setup(), "Host").unwrap();
        let addr = SocketAddr::from(([127, 0, 0, 1], host.port()));
        let mut client = NetClient::connect(addr, "Guest").unwrap();
        let wait = |f: &mut dyn FnMut() -> bool| {
            for _ in 0..200 {
                if f() {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            false
        };
        // Join and see the lobby.
        let mut lobby = None;
        assert!(wait(&mut || {
            host.poll(Instant::now(), None);
            host.send(None);
            for n in client.poll(Instant::now()) {
                if let ClientNote::Lobby(l) = n {
                    lobby = Some(l);
                }
            }
            lobby.is_some()
        }));
        let lobby = lobby.unwrap();
        assert_eq!(lobby.names, vec!["Host".to_string(), "Guest".to_string()]);
        assert_eq!(lobby.you, 1);

        // The host starts; the guest gets seat 1 and a snapshot.
        let mut race = Race::new(&t, host.racer_count(), true, 3);
        host.start(&mut race);
        assert!(race.racers[1].remote);
        let mut snap = None;
        assert!(wait(&mut || {
            host.poll(Instant::now(), Some(&mut race));
            host.send(Some(&race));
            for n in client.poll(Instant::now()) {
                if let ClientNote::Snapshot(s) = n {
                    snap = Some(s);
                }
            }
            snap.is_some()
        }));
        let snap = snap.unwrap();
        assert_eq!(snap.you, 1);
        assert_eq!(snap.names[1], "Guest");

        // The guest drives; the host sees the kart where the guest says it is, and a fire press.
        let mut client_race = Race::new(&t, snap.racers.len(), true, 3);
        let mut view = ClientView::new(1, snap.race_id, snap.racers.len());
        view.apply(&mut client_race, &snap);
        let mut moved = client_race.racers[1].state.clone();
        moved.position += Vec3::new(0.0, 0.0, 4.0);
        client.fires = 1;
        assert!(wait(&mut || {
            client.send_kart(&moved, &view, Instant::now());
            host.poll(Instant::now(), Some(&mut race));
            race.step(&t, DriveInput::default(), 1.0 / 60.0);
            race.racers[1].state.position == moved.position
        }));
    }

    #[test]
    fn snapshots_never_exceed_one_datagram() {
        let t = build_graybox_loop();
        let mut race = Race::new(&t, MAX_RACERS, true, 3);
        for r in &mut race.racers {
            r.name = rivet_net::clean_name(&"Жжжжжжжжжжжжжжжжжжжж".repeat(3)).into();
        }
        let mut combat = race.combat.net();
        combat.nails = vec![(Vec3::ONE, Vec3::X); 80];
        combat.orbs = vec![(Vec3::ONE, 0); 4];
        combat.hooks = vec![(0, Vec3::ONE, Some(1)); 8];
        let snap = Snapshot {
            combat,
            ..snapshot(&race, 1, setup(), 0, 1, vec![RaceEvent::Go; 200])
        };
        let bytes = snap.encode();
        assert!(bytes.len() <= MAX_PAYLOAD, "{} bytes", bytes.len());
        assert!(ToClient::decode(&bytes).is_ok());
    }

    #[test]
    fn hostnames_prefer_ipv4() {
        assert_eq!(
            resolve("localhost"),
            Some(SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT)))
        );
    }

    /// A UDP relay that delays every packet and drops some, between one client and the host.
    struct BadNetwork {
        sock: std::net::UdpSocket,
        host: SocketAddr,
        client: Option<SocketAddr>,
        queue: Vec<(Instant, SocketAddr, Vec<u8>)>,
        n: u32,
    }

    impl BadNetwork {
        fn new(host: SocketAddr) -> Self {
            let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            sock.set_nonblocking(true).unwrap();
            Self {
                sock,
                host,
                client: None,
                queue: Vec::new(),
                n: 0,
            }
        }

        fn addr(&self) -> SocketAddr {
            self.sock.local_addr().unwrap()
        }

        /// 80 ms each way, every 7th packet lost.
        fn pump(&mut self) {
            let mut buf = [0u8; 2048];
            while let Ok((len, from)) = self.sock.recv_from(&mut buf) {
                let to = if from == self.host {
                    match self.client {
                        Some(c) => c,
                        None => continue,
                    }
                } else {
                    self.client = Some(from);
                    self.host
                };
                self.n += 1;
                if self.n.is_multiple_of(7) {
                    continue;
                }
                let at = Instant::now() + std::time::Duration::from_millis(80);
                self.queue.push((at, to, buf[..len].to_vec()));
            }
            let now = Instant::now();
            let (due, later): (Vec<_>, Vec<_>) = self.queue.drain(..).partition(|q| q.0 <= now);
            self.queue = later;
            for (_, to, data) in due {
                let _ = self.sock.send_to(&data, to);
            }
        }
    }

    #[test]
    fn a_race_survives_latency_and_packet_loss() {
        let t = build_graybox_loop();
        let mut host = NetHost::bind(0, setup(), "Host").unwrap();
        let mut net = BadNetwork::new(SocketAddr::from(([127, 0, 0, 1], host.port())));
        let mut client = NetClient::connect(net.addr(), "Guest").unwrap();
        let mut race = Race::new(&t, 4, true, 3);
        let mut started = false;
        let mut client_race: Option<(Race, ClientView)> = None;
        let mut delivered: Vec<RaceEvent> = Vec::new();
        let mut sent: Vec<RaceEvent> = Vec::new();
        let mut moved_seen = false;
        let dt = 1.0 / 60.0;
        // Ten seconds of play in real time (the network delays are real).
        let t0 = Instant::now();
        let mut tick = 0u32;
        while t0.elapsed().as_secs_f32() < 10.0 {
            let now = Instant::now();
            net.pump();
            host.poll(now, started.then_some(&mut race));
            if !started && host.player_count() == 2 {
                race = Race::new(&t, host.racer_count(), true, 3);
                host.start(&mut race);
                started = true;
            }
            if started {
                let events = race.step(
                    &t,
                    DriveInput {
                        throttle: 1.0,
                        ..Default::default()
                    },
                    dt,
                );
                // Every other tick, a synthetic event so delivery is measurable.
                let mut events = events;
                if tick.is_multiple_of(2) {
                    events.push(RaceEvent::Lap {
                        racer: 0,
                        number: tick,
                        time: 0.0,
                    });
                }
                sent.extend(
                    events
                        .iter()
                        .copied()
                        .filter(|e| matches!(e, RaceEvent::Lap { racer: 0, .. })),
                );
                host.record(&events);
                if tick.is_multiple_of(2) {
                    host.send(Some(&race));
                }
            } else {
                host.send(None);
            }
            for note in client.poll(now) {
                if let ClientNote::Snapshot(snap) = note {
                    if client_race.is_none() {
                        let r = Race::new(&t, snap.racers.len(), true, 3);
                        client_race = Some((
                            r,
                            ClientView::new(snap.you as usize, snap.race_id, snap.racers.len()),
                        ));
                    }
                    if let Some((r, v)) = client_race.as_mut() {
                        delivered.extend(
                            v.apply(r, &snap)
                                .into_iter()
                                .filter(|e| matches!(e, RaceEvent::Lap { racer: 0, .. })),
                        );
                    }
                }
            }
            if let Some((r, v)) = client_race.as_mut() {
                let you = v.you;
                let mut s = r.racers[you].state.clone();
                s.position.z += 0.05 * tick as f32 / 60.0;
                r.racers[you].state = s.clone();
                client.send_kart(&s, v, now);
                if started && race.racers[you].state.position.distance(s.position) < 1.0 {
                    moved_seen = true;
                }
            }
            tick += 1;
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        assert!(started, "the guest joined through the bad network");
        assert!(moved_seen, "the host saw the guest's kart");
        // Every event except the last few still in flight arrived exactly once, in order.
        assert!(sent.len() > 100, "{} events sent", sent.len());
        let in_flight = 20;
        assert!(
            delivered.len() + in_flight >= sent.len(),
            "delivered {} of {}",
            delivered.len(),
            sent.len()
        );
        assert_eq!(
            &sent[..delivered.len()],
            &delivered[..],
            "no losses, repeats or reordering"
        );
    }

    #[test]
    fn tailscale_status_lists_online_peers() {
        let json = r#"{"Self":{"HostName":"me"},"Peer":{
            "a":{"HostName":"thinkpad","TailscaleIPs":["100.64.0.2","fd7a::2"],"Online":true},
            "b":{"HostName":"nas","TailscaleIPs":["100.64.0.3"],"Online":false},
            "c":{"HostName":"desk\u0007top","TailscaleIPs":["100.64.0.4"],"Online":true}}}"#;
        let peers = parse_tailscale_status(json);
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].name, "DESKTOP");
        assert_eq!(peers[1].name, "THINKPAD");
        assert_eq!(peers[1].ip.to_string(), "100.64.0.2");
        assert!(parse_tailscale_status("not json").is_empty());
    }

    #[test]
    fn addresses_resolve_with_the_default_port() {
        assert_eq!(
            resolve("127.0.0.1"),
            Some(SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT)))
        );
        assert_eq!(
            resolve("127.0.0.1:4000"),
            Some(SocketAddr::from(([127, 0, 0, 1], 4000)))
        );
        assert_eq!(resolve("localhost").map(|a| a.port()), Some(DEFAULT_PORT));
        assert_eq!(resolve("  "), None);
    }
}
