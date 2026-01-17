//! RAFT node state definitions

use serde::{Deserialize, Serialize};

/// The three possible states of a RAFT node
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRole {
    /// Passive node that responds to requests from leaders and candidates
    Follower,
    /// Node actively seeking votes to become leader
    Candidate,
    /// Node that handles all client requests and log replication
    Leader,
}

impl Default for NodeRole {
    fn default() -> Self {
        NodeRole::Follower
    }
}

/// Persistent state that must be saved to stable storage before responding to RPCs
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistentState {
    /// Latest term server has seen (initialized to 0, increases monotonically)
    pub current_term: u64,
    /// Candidate ID that received vote in current term (or None)
    pub voted_for: Option<heapless::String<64>>,
}

impl Default for PersistentState {
    fn default() -> Self {
        Self {
            current_term: 0,
            voted_for: None,
        }
    }
}

/// Volatile state on all servers
#[derive(Clone, Debug, Default)]
pub struct VolatileState {
    /// Index of highest log entry known to be committed (initialized to 0)
    pub commit_index: u64,
    /// Index of highest log entry applied to state machine (initialized to 0)
    pub last_applied: u64,
}

/// Volatile state on leaders (reinitialized after election)
#[derive(Clone, Debug)]
pub struct LeaderState {
    /// For each server, index of the next log entry to send to that server
    /// (initialized to leader last log index + 1)
    pub next_index: heapless::FnvIndexMap<heapless::String<64>, u64, 16>,
    /// For each server, index of highest log entry known to be replicated
    /// (initialized to 0)
    pub match_index: heapless::FnvIndexMap<heapless::String<64>, u64, 16>,
}

impl Default for LeaderState {
    fn default() -> Self {
        Self {
            next_index: heapless::FnvIndexMap::new(),
            match_index: heapless::FnvIndexMap::new(),
        }
    }
}

/// Information about a peer node in the cluster
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Unique identifier for this peer
    pub id: heapless::String<64>,
    /// Network address (IP:port)
    pub address: heapless::String<64>,
}

impl PeerInfo {
    /// Create a new peer info
    pub fn new(id: &str, address: &str) -> Self {
        let mut id_str = heapless::String::new();
        let _ = id_str.push_str(id);
        let mut addr_str = heapless::String::new();
        let _ = addr_str.push_str(address);
        Self {
            id: id_str,
            address: addr_str,
        }
    }
}

/// Cluster configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// List of all peers in the cluster (including self)
    pub peers: heapless::Vec<PeerInfo, 16>,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            peers: heapless::Vec::new(),
        }
    }
}
