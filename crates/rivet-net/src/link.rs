//! One host, a few clients, unreliable datagrams.
//!
//! Every packet starts with a 4-byte magic, a protocol version and a kind. A client sends
//! `Connect` (game id + player name) until the host answers `Accept` (its slot) or `Reject`.
//! After that both sides exchange `Data` packets; anything silent for [`TIMEOUT`] is dropped.
//! Packets from unknown addresses, wrong versions or malformed headers are ignored.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use crate::wire::{Reader, Writer};

const MAGIC: [u8; 4] = *b"RVNT";
const VERSION: u16 = 1;
/// Largest payload per packet (keeps datagrams under a typical path MTU).
pub const MAX_PAYLOAD: usize = 1200;
/// Silence after which a peer is considered gone.
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// How often a connecting client repeats its request.
const RETRY: Duration = Duration::from_millis(250);
/// How long a client keeps trying before giving up.
const GIVE_UP: Duration = Duration::from_secs(10);
/// Idle keepalive interval.
const KEEPALIVE: Duration = Duration::from_millis(500);
/// Connect requests are padded to at least this size, so no reply to an unverified address is
/// ever larger than the request (no reflection amplification).
const MIN_CONNECT: usize = 64;
/// Datagrams handled per poll, so a flood can't stall the frame.
const MAX_PER_POLL: usize = 256;
/// Longest player name kept, in characters.
const MAX_NAME: usize = 16;

/// A printable, trimmed, length-limited player name.
pub fn clean_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| !c.is_control()).collect();
    let s: String = s.trim().chars().take(MAX_NAME).collect();
    if s.is_empty() {
        "PLAYER".to_string()
    } else {
        s
    }
}

/// A hard-to-guess session token (std's per-process random hasher keys, mixed with the address
/// and time).
fn session_token(addr: SocketAddr, now: Instant) -> u64 {
    use std::hash::{BuildHasher, Hash, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    addr.hash(&mut h);
    now.hash(&mut h);
    h.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Connect = 0,
    Accept = 1,
    Reject = 2,
    Data = 3,
    Disconnect = 4,
    Ping = 5,
}

impl Kind {
    fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Kind::Connect,
            1 => Kind::Accept,
            2 => Kind::Reject,
            3 => Kind::Data,
            4 => Kind::Disconnect,
            5 => Kind::Ping,
            _ => return None,
        })
    }
}

fn packet(kind: Kind) -> Writer {
    let mut w = Writer::new();
    for b in MAGIC {
        w.u8(b);
    }
    w.u16(VERSION).u8(kind as u8);
    w
}

/// Splits a datagram into its kind and body, or `None` if it isn't ours.
fn parse(buf: &[u8]) -> Option<(Kind, Reader<'_>)> {
    let mut r = Reader::new(buf);
    let magic = [r.u8().ok()?, r.u8().ok()?, r.u8().ok()?, r.u8().ok()?];
    if magic != MAGIC || r.u16().ok()? != VERSION {
        return None;
    }
    let kind = Kind::from_u8(r.u8().ok()?)?;
    Some((kind, r))
}

fn too_big() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "payload larger than MAX_PAYLOAD",
    )
}

/// A connected client's slot on the host (0-based, stable while connected).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerId(pub u8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    Joined { peer: PeerId, name: String },
    Left { peer: PeerId },
    Message { peer: PeerId, data: Vec<u8> },
}

#[derive(Debug)]
struct Peer {
    addr: SocketAddr,
    token: u64,
    name: String,
    last_heard: Instant,
}

/// The authoritative side: accepts up to `max_peers` clients of one game.
#[derive(Debug)]
pub struct Host {
    sock: UdpSocket,
    game: u32,
    peers: Vec<Option<Peer>>,
}

impl Host {
    /// Listens on `addr` (e.g. `0.0.0.0:27960`; port 0 picks a free one).
    pub fn bind(addr: SocketAddr, max_peers: usize, game: u32) -> io::Result<Self> {
        let sock = UdpSocket::bind(addr)?;
        sock.set_nonblocking(true)?;
        Ok(Self {
            sock,
            game,
            peers: (0..max_peers).map(|_| None).collect(),
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sock.local_addr()
    }

    /// Connected peers and their names.
    pub fn peers(&self) -> impl Iterator<Item = (PeerId, &str)> {
        self.peers
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.as_ref().map(|p| (PeerId(i as u8), p.name.as_str())))
    }

    fn slot_of(&self, addr: SocketAddr) -> Option<usize> {
        self.peers
            .iter()
            .position(|p| p.as_ref().is_some_and(|p| p.addr == addr))
    }

    fn reply(&self, addr: SocketAddr, w: Writer) {
        // Unreliable by design: a lost reply is repeated when the client retries.
        let _ = self.sock.send_to(&w.finish(), addr);
    }

    /// Drains the socket (up to a per-call cap) and expires silent peers.
    pub fn poll(&mut self, now: Instant) -> Vec<HostEvent> {
        let mut events = Vec::new();
        let mut buf = [0u8; 2048];
        let mut errors = 0;
        for _ in 0..MAX_PER_POLL {
            let (n, from) = match self.sock.recv_from(&mut buf) {
                Ok(v) => v,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                // ICMP "port unreachable" from a vanished peer surfaces here on some systems.
                Err(_) => {
                    errors += 1;
                    if errors > 8 {
                        break;
                    }
                    continue;
                }
            };
            let Some((kind, mut body)) = parse(&buf[..n]) else {
                continue;
            };
            let known = self.slot_of(from);
            if kind == Kind::Connect {
                if n < MIN_CONNECT {
                    continue;
                }
                if let Some(i) = known {
                    let token = self.peers[i].as_ref().map_or(0, |p| p.token);
                    let mut w = packet(Kind::Accept);
                    w.u8(i as u8).u64(token);
                    self.reply(from, w);
                    continue;
                }
                let (Ok(game), Ok(name)) = (body.u32(), body.str()) else {
                    continue;
                };
                if game != self.game {
                    let mut w = packet(Kind::Reject);
                    w.str("different game or version");
                    self.reply(from, w);
                    continue;
                }
                let Some(i) = self.peers.iter().position(Option::is_none) else {
                    let mut w = packet(Kind::Reject);
                    w.str("server full");
                    self.reply(from, w);
                    continue;
                };
                let name = clean_name(name);
                let token = session_token(from, now);
                self.peers[i] = Some(Peer {
                    addr: from,
                    token,
                    name: name.clone(),
                    last_heard: now,
                });
                let mut w = packet(Kind::Accept);
                w.u8(i as u8).u64(token);
                self.reply(from, w);
                events.push(HostEvent::Joined {
                    peer: PeerId(i as u8),
                    name,
                });
                continue;
            }
            // Everything after the handshake carries the session token.
            let Some(i) = known else { continue };
            let Some(p) = self.peers[i].as_mut() else {
                continue;
            };
            if body.u64().ok() != Some(p.token) {
                continue;
            }
            p.last_heard = now;
            match kind {
                Kind::Data if body.rest().len() <= MAX_PAYLOAD => events.push(HostEvent::Message {
                    peer: PeerId(i as u8),
                    data: body.rest().to_vec(),
                }),
                Kind::Disconnect => {
                    self.peers[i] = None;
                    events.push(HostEvent::Left {
                        peer: PeerId(i as u8),
                    });
                }
                _ => {}
            }
        }
        for (i, slot) in self.peers.iter_mut().enumerate() {
            if slot
                .as_ref()
                .is_some_and(|p| now.saturating_duration_since(p.last_heard) > TIMEOUT)
            {
                *slot = None;
                events.push(HostEvent::Left {
                    peer: PeerId(i as u8),
                });
            }
        }
        events
    }

    /// Sends `data` to one peer (ignored if that slot is empty).
    pub fn send(&self, peer: PeerId, data: &[u8]) -> io::Result<()> {
        if data.len() > MAX_PAYLOAD {
            return Err(too_big());
        }
        if let Some(Some(p)) = self.peers.get(peer.0 as usize) {
            let mut w = packet(Kind::Data);
            w.u64(p.token);
            for &b in data {
                w.u8(b);
            }
            self.sock.send_to(&w.finish(), p.addr)?;
        }
        Ok(())
    }

    /// Removes a peer and tells it so.
    pub fn kick(&mut self, peer: PeerId) {
        if let Some(Some(p)) = self.peers.get(peer.0 as usize) {
            let mut w = packet(Kind::Disconnect);
            w.u64(p.token);
            self.reply(p.addr, w);
        }
        if let Some(slot) = self.peers.get_mut(peer.0 as usize) {
            *slot = None;
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        for p in self.peers.iter().flatten() {
            let mut w = packet(Kind::Disconnect);
            w.u64(p.token);
            let _ = self.sock.send_to(&w.finish(), p.addr);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientEvent {
    Connected {
        slot: u8,
    },
    Rejected {
        reason: String,
    },
    Message {
        data: Vec<u8>,
    },
    /// The host went silent, left, or never answered.
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientState {
    Connecting {
        started: Instant,
        last_try: Option<Instant>,
    },
    Connected {
        slot: u8,
        token: u64,
    },
    Closed,
}

/// The joining side.
#[derive(Debug)]
pub struct Client {
    sock: UdpSocket,
    server: SocketAddr,
    game: u32,
    name: String,
    state: ClientState,
    last_heard: Instant,
    last_sent: Instant,
}

impl Client {
    /// Starts connecting to `server` (the handshake runs inside [`Client::poll`]).
    pub fn connect(server: SocketAddr, name: &str, game: u32, now: Instant) -> io::Result<Self> {
        let any: SocketAddr = if server.is_ipv6() {
            "[::]:0".parse().expect("valid literal")
        } else {
            "0.0.0.0:0".parse().expect("valid literal")
        };
        let sock = UdpSocket::bind(any)?;
        sock.set_nonblocking(true)?;
        Ok(Self {
            sock,
            server,
            game,
            name: name.to_string(),
            state: ClientState::Connecting {
                started: now,
                last_try: None,
            },
            last_heard: now,
            last_sent: now,
        })
    }

    pub fn slot(&self) -> Option<u8> {
        match self.state {
            ClientState::Connected { slot, .. } => Some(slot),
            _ => None,
        }
    }

    pub fn server(&self) -> SocketAddr {
        self.server
    }

    fn raw(&mut self, w: Writer, now: Instant) {
        let _ = self.sock.send_to(&w.finish(), self.server);
        self.last_sent = now;
    }

    pub fn poll(&mut self, now: Instant) -> Vec<ClientEvent> {
        let mut events = Vec::new();
        if let ClientState::Connecting { started, last_try } = self.state {
            if now.saturating_duration_since(started) > GIVE_UP {
                self.state = ClientState::Closed;
                return vec![ClientEvent::Lost];
            }
            if last_try.is_none_or(|t| now.saturating_duration_since(t) >= RETRY) {
                let mut w = packet(Kind::Connect);
                w.u32(self.game).str(&self.name);
                while w.len() < MIN_CONNECT {
                    w.u8(0);
                }
                self.raw(w, now);
                self.state = ClientState::Connecting {
                    started,
                    last_try: Some(now),
                };
            }
        }
        let mut buf = [0u8; 2048];
        let mut errors = 0;
        for _ in 0..MAX_PER_POLL {
            let (n, from) = match self.sock.recv_from(&mut buf) {
                Ok(v) => v,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    errors += 1;
                    if errors > 8 {
                        break;
                    }
                    continue;
                }
            };
            if from != self.server || self.state == ClientState::Closed {
                continue;
            }
            let Some((kind, mut body)) = parse(&buf[..n]) else {
                continue;
            };
            // After the handshake, only packets with our session token count.
            if let ClientState::Connected { token, .. } = self.state
                && (!matches!(kind, Kind::Data | Kind::Disconnect)
                    || body.u64().ok() != Some(token))
            {
                continue;
            }
            self.last_heard = now;
            match kind {
                Kind::Accept => {
                    if let (ClientState::Connecting { .. }, Ok(slot), Ok(token)) =
                        (self.state, body.u8(), body.u64())
                    {
                        self.state = ClientState::Connected { slot, token };
                        events.push(ClientEvent::Connected { slot });
                    }
                }
                Kind::Reject => {
                    let reason = body.str().unwrap_or("rejected").to_string();
                    self.state = ClientState::Closed;
                    events.push(ClientEvent::Rejected { reason });
                }
                Kind::Data if self.slot().is_some() => events.push(ClientEvent::Message {
                    data: body.rest().to_vec(),
                }),
                Kind::Disconnect => {
                    self.state = ClientState::Closed;
                    events.push(ClientEvent::Lost);
                }
                _ => {}
            }
        }
        if self.slot().is_some() {
            if now.saturating_duration_since(self.last_heard) > TIMEOUT {
                self.state = ClientState::Closed;
                events.push(ClientEvent::Lost);
            } else if now.saturating_duration_since(self.last_sent) > KEEPALIVE {
                let w = self.tokened(Kind::Ping);
                self.raw(w, now);
            }
        }
        events
    }

    /// Sends `data` to the host (dropped until connected).
    pub fn send(&mut self, data: &[u8], now: Instant) -> io::Result<()> {
        if data.len() > MAX_PAYLOAD {
            return Err(too_big());
        }
        if self.slot().is_some() {
            let mut w = self.tokened(Kind::Data);
            for &b in data {
                w.u8(b);
            }
            self.raw(w, now);
        }
        Ok(())
    }

    fn tokened(&self, kind: Kind) -> Writer {
        let mut w = packet(kind);
        if let ClientState::Connected { token, .. } = self.state {
            w.u64(token);
        }
        w
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if self.slot().is_some() {
            let _ = self
                .sock
                .send_to(&self.tokened(Kind::Disconnect).finish(), self.server);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    const GAME: u32 = 0x4e41_494c;

    fn host(max: usize) -> (Host, SocketAddr) {
        let h = Host::bind("127.0.0.1:0".parse().unwrap(), max, GAME).unwrap();
        let addr = h.local_addr().unwrap();
        (h, addr)
    }

    /// Polls both sides until `done` or two seconds pass (loopback is fast, but asynchronous).
    fn pump(
        h: &mut Host,
        cs: &mut [&mut Client],
        mut done: impl FnMut(&[HostEvent], &[Vec<ClientEvent>]) -> bool,
    ) -> (Vec<HostEvent>, Vec<Vec<ClientEvent>>) {
        let (mut he, mut ce) = (Vec::new(), vec![Vec::new(); cs.len()]);
        for _ in 0..200 {
            let now = Instant::now();
            for (k, c) in cs.iter_mut().enumerate() {
                ce[k].extend(c.poll(now));
            }
            he.extend(h.poll(now));
            if done(&he, &ce) {
                break;
            }
            sleep(Duration::from_millis(10));
        }
        (he, ce)
    }

    #[test]
    fn connect_and_exchange_messages() {
        let (mut h, addr) = host(4);
        let mut c = Client::connect(addr, "Rusty", GAME, Instant::now()).unwrap();
        let (he, ce) = pump(&mut h, &mut [&mut c], |_, ce| {
            ce[0].contains(&ClientEvent::Connected { slot: 0 })
        });
        assert!(he.contains(&HostEvent::Joined {
            peer: PeerId(0),
            name: "Rusty".into()
        }));
        assert!(ce[0].contains(&ClientEvent::Connected { slot: 0 }));
        assert_eq!(h.peers().count(), 1);

        c.send(b"input", Instant::now()).unwrap();
        h.send(PeerId(0), b"snapshot").unwrap();
        let (he, ce) = pump(&mut h, &mut [&mut c], |he, ce| {
            !he.is_empty() && !ce[0].is_empty()
        });
        assert!(he.contains(&HostEvent::Message {
            peer: PeerId(0),
            data: b"input".to_vec()
        }));
        assert!(ce[0].contains(&ClientEvent::Message {
            data: b"snapshot".to_vec()
        }));
    }

    #[test]
    fn full_server_and_wrong_game_are_rejected() {
        let (mut h, addr) = host(1);
        let mut a = Client::connect(addr, "A", GAME, Instant::now()).unwrap();
        let mut b = Client::connect(addr, "B", GAME, Instant::now()).unwrap();
        let mut x = Client::connect(addr, "X", GAME + 1, Instant::now()).unwrap();
        let (_, ce) = pump(&mut h, &mut [&mut a, &mut b, &mut x], |_, ce| {
            ce.iter().all(|e| !e.is_empty())
        });
        let rejected = |e: &Vec<ClientEvent>| {
            e.iter().find_map(|e| match e {
                ClientEvent::Rejected { reason } => Some(reason.clone()),
                _ => None,
            })
        };
        assert!(ce[0].contains(&ClientEvent::Connected { slot: 0 }));
        assert_eq!(rejected(&ce[1]).as_deref(), Some("server full"));
        assert!(rejected(&ce[2]).unwrap().contains("different game"));
    }

    #[test]
    fn disconnect_frees_the_slot_and_silence_times_out() {
        let (mut h, addr) = host(2);
        let mut c = Client::connect(addr, "A", GAME, Instant::now()).unwrap();
        pump(&mut h, &mut [&mut c], |_, ce| !ce[0].is_empty());
        drop(c);
        let (he, _) = pump(&mut h, &mut [], |he, _| !he.is_empty());
        assert_eq!(he, vec![HostEvent::Left { peer: PeerId(0) }]);

        let mut c = Client::connect(addr, "B", GAME, Instant::now()).unwrap();
        pump(&mut h, &mut [&mut c], |_, ce| !ce[0].is_empty());
        assert_eq!(h.peers().count(), 1);
        // Pretend six seconds pass without a word from the client.
        let later = Instant::now() + Duration::from_secs(6);
        assert!(h.poll(later).contains(&HostEvent::Left { peer: PeerId(0) }));
        assert_eq!(h.peers().count(), 0);
        // And the client notices the host's silence too.
        assert!(c.poll(later).contains(&ClientEvent::Lost));
    }

    #[test]
    fn garbage_and_strangers_are_ignored() {
        let (mut h, addr) = host(2);
        let stranger = UdpSocket::bind("127.0.0.1:0").unwrap();
        stranger.send_to(b"hello", addr).unwrap();
        stranger
            .send_to(&[b'R', b'V', b'N', b'T', 1, 0, 9], addr)
            .unwrap();
        let mut data = packet(Kind::Data);
        data.u64(1).u8(1);
        stranger.send_to(&data.finish(), addr).unwrap();
        let mut bad_connect = packet(Kind::Connect);
        bad_connect.u8(1);
        stranger.send_to(&bad_connect.finish(), addr).unwrap();
        sleep(Duration::from_millis(50));
        assert!(h.poll(Instant::now()).is_empty());
        assert_eq!(h.peers().count(), 0);
    }

    #[test]
    fn spoofed_packets_without_the_session_token_are_ignored() {
        let (mut h, addr) = host(2);
        let mut c = Client::connect(addr, "A", GAME, Instant::now()).unwrap();
        pump(&mut h, &mut [&mut c], |_, ce| !ce[0].is_empty());
        // Same address as the client, wrong token: a forged Data and a forged Disconnect.
        for kind in [Kind::Data, Kind::Disconnect] {
            let mut w = packet(kind);
            w.u64(0xdead_beef);
            w.u8(9);
            c.sock.send_to(&w.finish(), addr).unwrap();
        }
        sleep(Duration::from_millis(50));
        assert!(h.poll(Instant::now()).is_empty());
        assert_eq!(h.peers().count(), 1, "still connected");
    }

    #[test]
    fn a_spoofed_reject_cannot_close_a_connected_client() {
        let (mut h, addr) = host(2);
        let mut c = Client::connect(addr, "A", GAME, Instant::now()).unwrap();
        pump(&mut h, &mut [&mut c], |_, ce| !ce[0].is_empty());
        let mut w = packet(Kind::Reject);
        w.str("gotcha");
        h.sock
            .send_to(&w.finish(), c.sock.local_addr().unwrap())
            .unwrap();
        sleep(Duration::from_millis(30));
        assert!(c.poll(Instant::now()).is_empty());
        assert!(c.slot().is_some());
    }

    #[test]
    fn short_connect_requests_get_no_reply() {
        let (mut h, addr) = host(2);
        let s = UdpSocket::bind("127.0.0.1:0").unwrap();
        s.set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut w = packet(Kind::Connect);
        w.u32(GAME + 1).str("x");
        s.send_to(&w.finish(), addr).unwrap();
        sleep(Duration::from_millis(30));
        h.poll(Instant::now());
        let mut buf = [0u8; 64];
        assert!(
            s.recv_from(&mut buf).is_err(),
            "no reply to an unpadded request"
        );
    }

    #[test]
    fn player_names_are_sanitized() {
        assert_eq!(clean_name("  Rusty\n\u{7}  "), "Rusty");
        assert_eq!(clean_name("\t"), "PLAYER");
        assert_eq!(clean_name(&"x".repeat(40)).len(), 16);
    }

    #[test]
    fn unanswered_client_gives_up() {
        let dead = UdpSocket::bind("127.0.0.1:0").unwrap();
        let t0 = Instant::now();
        let mut c = Client::connect(dead.local_addr().unwrap(), "A", GAME, t0).unwrap();
        assert!(c.poll(t0).is_empty());
        assert_eq!(
            c.poll(t0 + Duration::from_secs(11)),
            vec![ClientEvent::Lost]
        );
    }

    #[test]
    fn oversized_payloads_are_refused() {
        let (h, addr) = host(1);
        let mut c = Client::connect(addr, "A", GAME, Instant::now()).unwrap();
        let big = vec![0u8; MAX_PAYLOAD + 1];
        assert!(c.send(&big, Instant::now()).is_err());
        assert!(h.send(PeerId(0), &big).is_err());
    }
}
