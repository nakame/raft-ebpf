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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_role_default() {
        let role = NodeRole::default();
        assert_eq!(role, NodeRole::Follower);
    }

    #[test]
    fn test_node_role_equality() {
        assert_eq!(NodeRole::Follower, NodeRole::Follower);
        assert_eq!(NodeRole::Candidate, NodeRole::Candidate);
        assert_eq!(NodeRole::Leader, NodeRole::Leader);
        assert_ne!(NodeRole::Follower, NodeRole::Candidate);
        assert_ne!(NodeRole::Candidate, NodeRole::Leader);
        assert_ne!(NodeRole::Leader, NodeRole::Follower);
    }

    #[test]
    fn test_node_role_serialization() {
        let role = NodeRole::Leader;
        let json = serde_json::to_string(&role).expect("serialize");
        let parsed: NodeRole = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, NodeRole::Leader);
    }

    #[test]
    fn test_persistent_state_default() {
        let state = PersistentState::default();
        assert_eq!(state.current_term, 0);
        assert!(state.voted_for.is_none());
    }

    #[test]
    fn test_persistent_state_with_vote() {
        let mut voted_for = heapless::String::new();
        let _ = voted_for.push_str("candidate-01");
        let state = PersistentState {
            current_term: 5,
            voted_for: Some(voted_for),
        };
        assert_eq!(state.current_term, 5);
        assert_eq!(state.voted_for.as_ref().map(|s| s.as_str()), Some("candidate-01"));
    }

    #[test]
    fn test_persistent_state_serialization() {
        let mut voted_for = heapless::String::new();
        let _ = voted_for.push_str("node-02");
        let state = PersistentState {
            current_term: 10,
            voted_for: Some(voted_for),
        };
        let json = serde_json::to_string(&state).expect("serialize");
        let parsed: PersistentState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.current_term, 10);
        assert_eq!(
            parsed.voted_for.as_ref().map(|s: &heapless::String<64>| s.as_str()),
            Some("node-02")
        );
    }

    #[test]
    fn test_volatile_state_default() {
        let state = VolatileState::default();
        assert_eq!(state.commit_index, 0);
        assert_eq!(state.last_applied, 0);
    }

    #[test]
    fn test_volatile_state_modification() {
        let mut state = VolatileState::default();
        state.commit_index = 5;
        state.last_applied = 4;
        assert_eq!(state.commit_index, 5);
        assert_eq!(state.last_applied, 4);
    }

    #[test]
    fn test_leader_state_default() {
        let state = LeaderState::default();
        assert!(state.next_index.is_empty());
        assert!(state.match_index.is_empty());
    }

    #[test]
    fn test_leader_state_peer_tracking() {
        let mut state = LeaderState::default();
        let mut peer_id = heapless::String::new();
        let _ = peer_id.push_str("follower-01");

        let _ = state.next_index.insert(peer_id.clone(), 10);
        let _ = state.match_index.insert(peer_id.clone(), 8);

        assert_eq!(state.next_index.get(&peer_id), Some(&10));
        assert_eq!(state.match_index.get(&peer_id), Some(&8));
    }

    #[test]
    fn test_leader_state_multiple_peers() {
        let mut state = LeaderState::default();

        let peer_names = ["peer-0", "peer-1", "peer-2", "peer-3", "peer-4"];
        for (i, name) in peer_names.iter().enumerate() {
            let mut peer_id = heapless::String::new();
            let _ = peer_id.push_str(name);
            let _ = state.next_index.insert(peer_id.clone(), 10 + i as u64);
            let _ = state.match_index.insert(peer_id, 5 + i as u64);
        }

        assert_eq!(state.next_index.len(), 5);
        assert_eq!(state.match_index.len(), 5);
    }

    #[test]
    fn test_peer_info_creation() {
        let peer = PeerInfo::new("node-01", "192.168.1.10:5555");
        assert_eq!(peer.id.as_str(), "node-01");
        assert_eq!(peer.address.as_str(), "192.168.1.10:5555");
    }

    #[test]
    fn test_peer_info_serialization() {
        let peer = PeerInfo::new("node-02", "10.0.0.2:5555");
        let json = serde_json::to_string(&peer).expect("serialize");
        let parsed: PeerInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.id.as_str(), "node-02");
        assert_eq!(parsed.address.as_str(), "10.0.0.2:5555");
    }

    #[test]
    fn test_cluster_config_default() {
        let config = ClusterConfig::default();
        assert!(config.peers.is_empty());
    }

    #[test]
    fn test_cluster_config_with_peers() {
        let mut config = ClusterConfig::default();
        let _ = config.peers.push(PeerInfo::new("node-01", "192.168.1.1:5555"));
        let _ = config.peers.push(PeerInfo::new("node-02", "192.168.1.2:5555"));
        let _ = config.peers.push(PeerInfo::new("node-03", "192.168.1.3:5555"));

        assert_eq!(config.peers.len(), 3);
        assert_eq!(config.peers[0].id.as_str(), "node-01");
        assert_eq!(config.peers[1].id.as_str(), "node-02");
        assert_eq!(config.peers[2].id.as_str(), "node-03");
    }

    #[test]
    fn test_cluster_config_serialization() {
        let mut config = ClusterConfig::default();
        let _ = config.peers.push(PeerInfo::new("node-01", "192.168.1.1:5555"));

        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: ClusterConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.peers.len(), 1);
        assert_eq!(parsed.peers[0].id.as_str(), "node-01");
    }
}
