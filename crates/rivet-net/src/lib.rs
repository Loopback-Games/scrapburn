#![forbid(unsafe_code)]
//! Rivet networking: unreliable UDP links between one host and a handful of clients, with a
//! connect handshake, a game id check and timeouts, plus a bounds-checked little-endian byte
//! codec for messages. Built for state synchronization: every message is a repeatable snapshot
//! of state, so no reliability layer is needed.

pub mod link;
pub mod wire;

pub use link::{Client, ClientEvent, Host, HostEvent, MAX_PAYLOAD, PeerId, clean_name};
pub use wire::{Reader, WireError, Writer};
