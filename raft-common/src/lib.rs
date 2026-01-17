//! Shared types for RAFT consensus implementation
//!
//! This crate provides common data structures used by both the eBPF kernel
//! program and the userspace RAFT daemon.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod maps;
pub mod protocol;
pub mod state;

pub use maps::*;
pub use protocol::*;
pub use state::*;

/// Default RAFT communication port
pub const RAFT_PORT: u16 = 5555;

/// Maximum number of peers in a cluster
pub const MAX_PEERS: usize = 16;

/// Maximum size of a log entry value in bytes
pub const MAX_VALUE_SIZE: usize = 1024;

/// Maximum size of a key in bytes
pub const MAX_KEY_SIZE: usize = 256;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(RAFT_PORT, 5555);
        assert_eq!(MAX_PEERS, 16);
        assert_eq!(MAX_VALUE_SIZE, 1024);
        assert_eq!(MAX_KEY_SIZE, 256);
    }
}
