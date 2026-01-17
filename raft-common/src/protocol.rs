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
