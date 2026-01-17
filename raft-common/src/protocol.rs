//! RAFT protocol message definitions
//!
//! Implements the core RAFT RPC message types as defined in the RAFT paper.

use serde::{Deserialize, Serialize};

/// A single entry in the RAFT log
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(not(feature = "std"), derive(Default))]
pub struct LogEntry {
    /// Position in the log (1-indexed)
    pub index: u64,
    /// Term when entry was received by leader
    pub term: u64,
    /// The command to apply to the state machine
    pub command: Command,
}

/// Commands that can be applied to the state machine
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Command {
    /// Store a key-value pair
    Put {
        key: heapless::String<256>,
        value: heapless::Vec<u8, 1024>,
    },
    /// Delete a key
    Delete { key: heapless::String<256> },
    /// No-op command (used for leader commit)
    Noop,
}

#[cfg(not(feature = "std"))]
impl Default for Command {
    fn default() -> Self {
        Command::Noop
    }
}

/// RAFT protocol message types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RaftMessage {
    /// AppendEntries RPC (heartbeat and log replication)
    AppendEntries(AppendEntriesRequest),
    /// Response to AppendEntries
    AppendEntriesResponse(AppendEntriesResponse),
    /// RequestVote RPC (leader election)
    RequestVote(RequestVoteRequest),
    /// Response to RequestVote
    RequestVoteResponse(RequestVoteResponse),
    /// Client request to propose a command
    ClientRequest(ClientRequest),
    /// Response to client request
    ClientResponse(ClientResponse),
}

/// AppendEntries RPC request
///
/// Invoked by leader to replicate log entries; also used as heartbeat.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppendEntriesRequest {
    /// Leader's term
    pub term: u64,
    /// Leader's ID so follower can redirect clients
    pub leader_id: heapless::String<64>,
    /// Index of log entry immediately preceding new ones
    pub prev_log_index: u64,
    /// Term of prev_log_index entry
    pub prev_log_term: u64,
    /// Log entries to store (empty for heartbeat)
    pub entries: heapless::Vec<LogEntry, 64>,
    /// Leader's commit index
    pub leader_commit: u64,
}

/// AppendEntries RPC response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppendEntriesResponse {
    /// Current term, for leader to update itself
    pub term: u64,
    /// True if follower contained entry matching prev_log_index and prev_log_term
    pub success: bool,
    /// The index of the last log entry (for fast backtracking)
    pub last_log_index: u64,
    /// Responder's node ID
    pub node_id: heapless::String<64>,
}

/// RequestVote RPC request
///
/// Invoked by candidates to gather votes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestVoteRequest {
    /// Candidate's term
    pub term: u64,
    /// Candidate requesting vote
    pub candidate_id: heapless::String<64>,
    /// Index of candidate's last log entry
    pub last_log_index: u64,
    /// Term of candidate's last log entry
    pub last_log_term: u64,
}

/// RequestVote RPC response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestVoteResponse {
    /// Current term, for candidate to update itself
    pub term: u64,
    /// True means candidate received vote
    pub vote_granted: bool,
    /// Responder's node ID
    pub node_id: heapless::String<64>,
}

/// Client request to the RAFT cluster
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientRequest {
    /// Unique request ID for deduplication
    pub request_id: u64,
    /// The command to execute
    pub command: Command,
}

/// Response to a client request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientResponse {
    /// Whether the request was successful
    pub success: bool,
    /// The request ID this is responding to
    pub request_id: u64,
    /// Result data (for Get operations)
    pub data: Option<heapless::Vec<u8, 1024>>,
    /// Error message if not successful
    pub error: Option<heapless::String<256>>,
    /// Current leader ID (for redirects)
    pub leader_id: Option<heapless::String<64>>,
}

/// Message envelope for network transmission
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageEnvelope {
    /// Sender's node ID
    pub from: heapless::String<64>,
    /// Recipient's node ID (empty for broadcast)
    pub to: heapless::String<64>,
    /// The actual message
    pub message: RaftMessage,
}

impl MessageEnvelope {
    /// Create a new message envelope
    pub fn new(
        from: &str,
        to: &str,
        message: RaftMessage,
    ) -> Self {
        let mut from_str = heapless::String::new();
        let _ = from_str.push_str(from);
        let mut to_str = heapless::String::new();
        let _ = to_str.push_str(to);
        Self {
            from: from_str,
            to: to_str,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(s: &str) -> heapless::String<256> {
        let mut hs = heapless::String::new();
        let _ = hs.push_str(s);
        hs
    }

    fn make_value(data: &[u8]) -> heapless::Vec<u8, 1024> {
        let mut hv = heapless::Vec::new();
        for b in data {
            let _ = hv.push(*b);
        }
        hv
    }

    fn make_node_id(s: &str) -> heapless::String<64> {
        let mut hs = heapless::String::new();
        let _ = hs.push_str(s);
        hs
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry {
            index: 1,
            term: 5,
            command: Command::Noop,
        };
        assert_eq!(entry.index, 1);
        assert_eq!(entry.term, 5);
    }

    #[test]
    fn test_command_put() {
        let cmd = Command::Put {
            key: make_key("test_key"),
            value: make_value(b"test_value"),
        };
        if let Command::Put { key, value } = cmd {
            assert_eq!(key.as_str(), "test_key");
            assert_eq!(value.as_slice(), b"test_value");
        } else {
            panic!("Expected Put command");
        }
    }

    #[test]
    fn test_command_delete() {
        let cmd = Command::Delete {
            key: make_key("delete_key"),
        };
        if let Command::Delete { key } = cmd {
            assert_eq!(key.as_str(), "delete_key");
        } else {
            panic!("Expected Delete command");
        }
    }

    #[test]
    fn test_command_noop() {
        let cmd = Command::Noop;
        assert!(matches!(cmd, Command::Noop));
    }

    #[test]
    fn test_append_entries_request() {
        let req = AppendEntriesRequest {
            term: 10,
            leader_id: make_node_id("leader-01"),
            prev_log_index: 5,
            prev_log_term: 9,
            entries: heapless::Vec::new(),
            leader_commit: 4,
        };
        assert_eq!(req.term, 10);
        assert_eq!(req.leader_id.as_str(), "leader-01");
        assert_eq!(req.prev_log_index, 5);
        assert_eq!(req.prev_log_term, 9);
        assert!(req.entries.is_empty());
        assert_eq!(req.leader_commit, 4);
    }

    #[test]
    fn test_append_entries_response() {
        let resp = AppendEntriesResponse {
            term: 10,
            success: true,
            last_log_index: 6,
            node_id: make_node_id("follower-01"),
        };
        assert_eq!(resp.term, 10);
        assert!(resp.success);
        assert_eq!(resp.last_log_index, 6);
        assert_eq!(resp.node_id.as_str(), "follower-01");
    }

    #[test]
    fn test_request_vote_request() {
        let req = RequestVoteRequest {
            term: 15,
            candidate_id: make_node_id("candidate-01"),
            last_log_index: 10,
            last_log_term: 14,
        };
        assert_eq!(req.term, 15);
        assert_eq!(req.candidate_id.as_str(), "candidate-01");
        assert_eq!(req.last_log_index, 10);
        assert_eq!(req.last_log_term, 14);
    }

    #[test]
    fn test_request_vote_response() {
        let resp = RequestVoteResponse {
            term: 15,
            vote_granted: true,
            node_id: make_node_id("voter-01"),
        };
        assert_eq!(resp.term, 15);
        assert!(resp.vote_granted);
        assert_eq!(resp.node_id.as_str(), "voter-01");
    }

    #[test]
    fn test_client_request() {
        let req = ClientRequest {
            request_id: 12345,
            command: Command::Noop,
        };
        assert_eq!(req.request_id, 12345);
        assert!(matches!(req.command, Command::Noop));
    }

    #[test]
    fn test_client_response_success() {
        let resp = ClientResponse {
            success: true,
            request_id: 12345,
            data: Some(make_value(b"result")),
            error: None,
            leader_id: None,
        };
        assert!(resp.success);
        assert_eq!(resp.request_id, 12345);
        assert!(resp.data.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_client_response_failure() {
        let mut error_msg = heapless::String::<256>::new();
        let _ = error_msg.push_str("Not leader");
        let resp = ClientResponse {
            success: false,
            request_id: 12345,
            data: None,
            error: Some(error_msg),
            leader_id: Some(make_node_id("leader-02")),
        };
        assert!(!resp.success);
        assert!(resp.error.is_some());
        assert_eq!(resp.leader_id.as_ref().map(|s| s.as_str()), Some("leader-02"));
    }

    #[test]
    fn test_message_envelope_creation() {
        let msg = RaftMessage::RequestVote(RequestVoteRequest {
            term: 1,
            candidate_id: make_node_id("node-01"),
            last_log_index: 0,
            last_log_term: 0,
        });
        let envelope = MessageEnvelope::new("node-01", "node-02", msg);
        assert_eq!(envelope.from.as_str(), "node-01");
        assert_eq!(envelope.to.as_str(), "node-02");
    }

    #[test]
    fn test_raft_message_variants() {
        // Test all RaftMessage variants can be created
        let _append = RaftMessage::AppendEntries(AppendEntriesRequest {
            term: 1,
            leader_id: make_node_id("leader"),
            prev_log_index: 0,
            prev_log_term: 0,
            entries: heapless::Vec::new(),
            leader_commit: 0,
        });

        let _append_resp = RaftMessage::AppendEntriesResponse(AppendEntriesResponse {
            term: 1,
            success: true,
            last_log_index: 0,
            node_id: make_node_id("follower"),
        });

        let _vote = RaftMessage::RequestVote(RequestVoteRequest {
            term: 1,
            candidate_id: make_node_id("candidate"),
            last_log_index: 0,
            last_log_term: 0,
        });

        let _vote_resp = RaftMessage::RequestVoteResponse(RequestVoteResponse {
            term: 1,
            vote_granted: true,
            node_id: make_node_id("voter"),
        });

        let _client_req = RaftMessage::ClientRequest(ClientRequest {
            request_id: 1,
            command: Command::Noop,
        });

        let _client_resp = RaftMessage::ClientResponse(ClientResponse {
            success: true,
            request_id: 1,
            data: None,
            error: None,
            leader_id: None,
        });
    }

    #[test]
    fn test_log_entry_serialization() {
        let entry = LogEntry {
            index: 42,
            term: 7,
            command: Command::Put {
                key: make_key("mykey"),
                value: make_value(b"myvalue"),
            },
        };

        let json = serde_json::to_string(&entry).expect("serialize");
        let parsed: LogEntry = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.index, 42);
        assert_eq!(parsed.term, 7);
        if let Command::Put { key, value } = parsed.command {
            assert_eq!(key.as_str(), "mykey");
            assert_eq!(value.as_slice(), b"myvalue");
        } else {
            panic!("Expected Put command after deserialization");
        }
    }

    #[test]
    fn test_append_entries_with_entries() {
        let mut entries = heapless::Vec::<LogEntry, 64>::new();
        let _ = entries.push(LogEntry {
            index: 1,
            term: 1,
            command: Command::Noop,
        });
        let _ = entries.push(LogEntry {
            index: 2,
            term: 1,
            command: Command::Put {
                key: make_key("k"),
                value: make_value(b"v"),
            },
        });

        let req = AppendEntriesRequest {
            term: 1,
            leader_id: make_node_id("leader"),
            prev_log_index: 0,
            prev_log_term: 0,
            entries,
            leader_commit: 0,
        };

        assert_eq!(req.entries.len(), 2);
        assert_eq!(req.entries[0].index, 1);
        assert_eq!(req.entries[1].index, 2);
    }
}
