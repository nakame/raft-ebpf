//! Core RAFT node implementation

use crate::config::Config;
use crate::log::RaftLog;
use crate::state_machine::StateMachine;
use anyhow::Result;
use raft_common::{
    AppendEntriesRequest, AppendEntriesResponse, Command, LogEntry, MessageEnvelope, NodeRole,
    PeerInfo, RaftMessage, RequestVoteRequest, RequestVoteResponse,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, info, warn};

/// Commands that can be sent to the node
pub enum NodeCommand {
    /// Process an incoming RAFT message
    HandleMessage(MessageEnvelope),
    /// Propose a new command (from client)
    Propose(Command, mpsc::Sender<ProposeResult>),
    /// Trigger election timeout
    ElectionTimeout,
    /// Trigger heartbeat (leader only)
    Heartbeat,
    /// Add a peer to the cluster
    AddPeer(String, String),
    /// Remove a peer from the cluster
    RemovePeer(String),
    /// Get current node status
    GetStatus(mpsc::Sender<NodeStatus>),
    /// Shutdown the node
    Shutdown,
}

/// Result of a propose operation
#[derive(Debug, Clone)]
pub enum ProposeResult {
    /// Command was committed
    Success { index: u64 },
    /// Node is not the leader
    NotLeader { leader_id: Option<String> },
    /// Failed to replicate
    Failed { reason: String },
}

/// Current status of the node
#[derive(Debug, Clone)]
pub struct NodeStatus {
    pub node_id: String,
    pub role: NodeRole,
    pub current_term: u64,
    pub commit_index: u64,
    pub last_applied: u64,
    pub leader_id: Option<String>,
    pub peers: Vec<String>,
}

/// The main RAFT node
pub struct RaftNode {
    /// Node ID
    pub node_id: String,
    /// Configuration
    config: Config,
    /// Current role
    role: RwLock<NodeRole>,
    /// Persistent log
    log: Mutex<RaftLog>,
    /// State machine
    state_machine: Mutex<StateMachine>,
    /// Commit index
    commit_index: RwLock<u64>,
    /// Last applied index
    last_applied: RwLock<u64>,
    /// Current leader ID
    leader_id: RwLock<Option<String>>,
    /// Peers in the cluster
    peers: RwLock<HashMap<String, PeerInfo>>,
    /// Leader state: next_index for each peer
    next_index: RwLock<HashMap<String, u64>>,
    /// Leader state: match_index for each peer
    match_index: RwLock<HashMap<String, u64>>,
    /// Votes received (candidate state)
    votes_received: RwLock<Vec<String>>,
    /// Last heartbeat received
    last_heartbeat: RwLock<Instant>,
    /// Channel to send outgoing messages
    outgoing_tx: mpsc::Sender<MessageEnvelope>,
    /// Pending client requests (index -> response channel)
    pending_requests: Mutex<HashMap<u64, mpsc::Sender<ProposeResult>>>,
}

impl RaftNode {
    /// Create a new RAFT node
    pub async fn new(
        config: Config,
        outgoing_tx: mpsc::Sender<MessageEnvelope>,
    ) -> Result<Arc<Self>> {
        let data_dir = config.server.data_dir.join(&config.server.node_id);
        let log = RaftLog::new(data_dir)?;

        let mut peers = HashMap::new();
        for peer in &config.peers {
            let info = PeerInfo::new(&peer.id, &peer.address);
            peers.insert(peer.id.clone(), info);
        }

        let node = Arc::new(Self {
            node_id: config.server.node_id.clone(),
            config,
            role: RwLock::new(NodeRole::Follower),
            log: Mutex::new(log),
            state_machine: Mutex::new(StateMachine::new()),
            commit_index: RwLock::new(0),
            last_applied: RwLock::new(0),
            leader_id: RwLock::new(None),
            peers: RwLock::new(peers),
            next_index: RwLock::new(HashMap::new()),
            match_index: RwLock::new(HashMap::new()),
            votes_received: RwLock::new(Vec::new()),
            last_heartbeat: RwLock::new(Instant::now()),
            outgoing_tx,
            pending_requests: Mutex::new(HashMap::new()),
        });

        Ok(node)
    }

    /// Get current term
    pub async fn current_term(&self) -> u64 {
        self.log.lock().await.current_term()
    }

    /// Get current role
    pub async fn role(&self) -> NodeRole {
        self.role.read().await.clone()
    }

    /// Handle an incoming message
    pub async fn handle_message(&self, envelope: MessageEnvelope) -> Result<()> {
        match envelope.message {
            RaftMessage::AppendEntries(req) => {
                self.handle_append_entries(&envelope.from.as_str(), req)
                    .await?;
            }
            RaftMessage::AppendEntriesResponse(resp) => {
                self.handle_append_entries_response(&envelope.from.as_str(), resp)
                    .await?;
            }
            RaftMessage::RequestVote(req) => {
                self.handle_request_vote(&envelope.from.as_str(), req)
                    .await?;
            }
            RaftMessage::RequestVoteResponse(resp) => {
                self.handle_request_vote_response(&envelope.from.as_str(), resp)
                    .await?;
            }
            RaftMessage::ClientRequest(_) | RaftMessage::ClientResponse(_) => {
                // Handled separately
            }
        }
        Ok(())
    }

    /// Handle AppendEntries RPC
    async fn handle_append_entries(
        &self,
        from: &str,
        req: AppendEntriesRequest,
    ) -> Result<()> {
        let mut log = self.log.lock().await;
        let current_term = log.current_term();

        // Reply false if term < currentTerm
        if req.term < current_term {
            self.send_append_entries_response(from, current_term, false)
                .await?;
            return Ok(());
        }

        // Update term if needed
        if req.term > current_term {
            log.set_current_term(req.term)?;
            *self.role.write().await = NodeRole::Follower;
        }

        // Reset election timer
        *self.last_heartbeat.write().await = Instant::now();
        *self.leader_id.write().await = Some(req.leader_id.as_str().to_string());

        // Check log consistency
        if req.prev_log_index > 0 && !log.has_entry(req.prev_log_index, req.prev_log_term) {
            debug!(
                "Log inconsistency at index {}, term {}",
                req.prev_log_index, req.prev_log_term
            );
            self.send_append_entries_response(from, log.current_term(), false)
                .await?;
            return Ok(());
        }

        // Append new entries
        if !req.entries.is_empty() {
            // Delete conflicting entries
            for entry in &req.entries {
                if let Some(existing_term) = log.term_at(entry.index) {
                    if existing_term != entry.term {
                        log.truncate_from(entry.index)?;
                        break;
                    }
                }
            }

            // Append new entries
            log.append_entries(req.entries.to_vec())?;
        }

        // Update commit index
        if req.leader_commit > *self.commit_index.read().await {
            let last_new_entry = log.last_index();
            let new_commit = req.leader_commit.min(last_new_entry);
            *self.commit_index.write().await = new_commit;
        }

        // Apply committed entries
        drop(log);
        self.apply_committed_entries().await?;

        self.send_append_entries_response(from, current_term, true)
            .await?;
        Ok(())
    }

    /// Send AppendEntries response
    async fn send_append_entries_response(
        &self,
        to: &str,
        term: u64,
        success: bool,
    ) -> Result<()> {
        let log = self.log.lock().await;
        let mut node_id = heapless::String::new();
        let _ = node_id.push_str(&self.node_id);

        let response = AppendEntriesResponse {
            term,
            success,
            last_log_index: log.last_index(),
            node_id,
        };

        let envelope = MessageEnvelope::new(
            &self.node_id,
            to,
            RaftMessage::AppendEntriesResponse(response),
        );
        let _ = self.outgoing_tx.send(envelope).await;
        Ok(())
    }

    /// Handle AppendEntries response
    async fn handle_append_entries_response(
        &self,
        from: &str,
        resp: AppendEntriesResponse,
    ) -> Result<()> {
        let log = self.log.lock().await;

        // Ignore if not leader
        if *self.role.read().await != NodeRole::Leader {
            return Ok(());
        }

        // Step down if term is higher
        if resp.term > log.current_term() {
            drop(log);
            let mut log = self.log.lock().await;
            log.set_current_term(resp.term)?;
            *self.role.write().await = NodeRole::Follower;
            return Ok(());
        }

        if resp.success {
            // Update next_index and match_index
            let mut next_index = self.next_index.write().await;
            let mut match_index = self.match_index.write().await;

            let new_match = resp.last_log_index;
            match_index.insert(from.to_string(), new_match);
            next_index.insert(from.to_string(), new_match + 1);

            drop(next_index);
            drop(match_index);
            drop(log);

            // Try to advance commit index
            self.try_advance_commit_index().await?;
        } else {
            // Decrement next_index and retry
            let mut next_index = self.next_index.write().await;
            if let Some(idx) = next_index.get_mut(from) {
                *idx = (*idx).saturating_sub(1).max(1);
            }
        }

        Ok(())
    }

    /// Handle RequestVote RPC
    async fn handle_request_vote(&self, from: &str, req: RequestVoteRequest) -> Result<()> {
        let mut log = self.log.lock().await;
        let current_term = log.current_term();

        // Reply false if term < currentTerm
        if req.term < current_term {
            self.send_request_vote_response(from, current_term, false)
                .await?;
            return Ok(());
        }

        // Update term if needed
        if req.term > current_term {
            log.set_current_term(req.term)?;
            *self.role.write().await = NodeRole::Follower;
        }

        // Check if we can vote for this candidate
        let voted_for = log.voted_for();
        let can_vote = voted_for.is_none() || voted_for.as_deref() == Some(req.candidate_id.as_str());

        // Check log is at least as up-to-date
        let last_term = log.last_term();
        let last_index = log.last_index();
        let log_ok = req.last_log_term > last_term
            || (req.last_log_term == last_term && req.last_log_index >= last_index);

        if can_vote && log_ok {
            log.set_voted_for(Some(req.candidate_id.as_str()))?;
            *self.last_heartbeat.write().await = Instant::now();
            self.send_request_vote_response(from, log.current_term(), true)
                .await?;
        } else {
            self.send_request_vote_response(from, log.current_term(), false)
                .await?;
        }

        Ok(())
    }

    /// Send RequestVote response
    async fn send_request_vote_response(
        &self,
        to: &str,
        term: u64,
        vote_granted: bool,
    ) -> Result<()> {
        let mut node_id = heapless::String::new();
        let _ = node_id.push_str(&self.node_id);

        let response = RequestVoteResponse {
            term,
            vote_granted,
            node_id,
        };

        let envelope = MessageEnvelope::new(
            &self.node_id,
            to,
            RaftMessage::RequestVoteResponse(response),
        );
        let _ = self.outgoing_tx.send(envelope).await;
        Ok(())
    }

    /// Handle RequestVote response
    async fn handle_request_vote_response(
        &self,
        from: &str,
        resp: RequestVoteResponse,
    ) -> Result<()> {
        let log = self.log.lock().await;

        // Ignore if not candidate
        if *self.role.read().await != NodeRole::Candidate {
            return Ok(());
        }

        // Step down if term is higher
        if resp.term > log.current_term() {
            drop(log);
            let mut log = self.log.lock().await;
            log.set_current_term(resp.term)?;
            *self.role.write().await = NodeRole::Follower;
            return Ok(());
        }

        if resp.vote_granted {
            let mut votes = self.votes_received.write().await;
            if !votes.contains(&from.to_string()) {
                votes.push(from.to_string());
            }

            // Check if we have majority
            let peers = self.peers.read().await;
            let total_nodes = peers.len() + 1; // +1 for self
            let majority = total_nodes / 2 + 1;

            if votes.len() + 1 >= majority {
                // +1 for self-vote
                drop(votes);
                drop(peers);
                drop(log);
                self.become_leader().await?;
            }
        }

        Ok(())
    }

    /// Start an election
    pub async fn start_election(&self) -> Result<()> {
        let mut log = self.log.lock().await;
        let new_term = log.current_term() + 1;
        log.set_current_term(new_term)?;
        log.set_voted_for(Some(&self.node_id))?;

        *self.role.write().await = NodeRole::Candidate;
        *self.votes_received.write().await = vec![self.node_id.clone()];
        *self.last_heartbeat.write().await = Instant::now();

        info!(
            "Node {} starting election for term {}",
            self.node_id, new_term
        );

        let last_log_index = log.last_index();
        let last_log_term = log.last_term();

        let mut candidate_id = heapless::String::new();
        let _ = candidate_id.push_str(&self.node_id);

        let request = RequestVoteRequest {
            term: new_term,
            candidate_id,
            last_log_index,
            last_log_term,
        };

        // Send RequestVote to all peers
        let peers = self.peers.read().await;
        for (peer_id, _) in peers.iter() {
            let envelope = MessageEnvelope::new(
                &self.node_id,
                peer_id,
                RaftMessage::RequestVote(request.clone()),
            );
            let _ = self.outgoing_tx.send(envelope).await;
        }

        // Check if we're the only node (single-node cluster)
        if peers.is_empty() {
            drop(peers);
            drop(log);
            self.become_leader().await?;
        }

        Ok(())
    }

    /// Become leader
    async fn become_leader(&self) -> Result<()> {
        info!("Node {} became leader", self.node_id);
        *self.role.write().await = NodeRole::Leader;
        *self.leader_id.write().await = Some(self.node_id.clone());

        // Initialize next_index and match_index
        let log = self.log.lock().await;
        let last_index = log.last_index();
        drop(log);

        let peers = self.peers.read().await;
        let mut next_index = self.next_index.write().await;
        let mut match_index = self.match_index.write().await;

        for peer_id in peers.keys() {
            next_index.insert(peer_id.clone(), last_index + 1);
            match_index.insert(peer_id.clone(), 0);
        }

        drop(next_index);
        drop(match_index);
        drop(peers);

        // Send initial heartbeat
        self.send_heartbeats().await?;

        Ok(())
    }

    /// Send heartbeats to all followers
    pub async fn send_heartbeats(&self) -> Result<()> {
        if *self.role.read().await != NodeRole::Leader {
            return Ok(());
        }

        let log = self.log.lock().await;
        let current_term = log.current_term();
        let commit_index = *self.commit_index.read().await;

        let peers = self.peers.read().await;
        let next_index = self.next_index.read().await;

        for (peer_id, _) in peers.iter() {
            let next_idx = next_index.get(peer_id).copied().unwrap_or(1);
            let prev_log_index = next_idx.saturating_sub(1);
            let prev_log_term = log.term_at(prev_log_index).unwrap_or(0);

            // Get entries to send
            let entries: heapless::Vec<LogEntry, 64> = log
                .get_entries(next_idx, log.last_index())
                .into_iter()
                .take(64)
                .collect();

            let mut leader_id = heapless::String::new();
            let _ = leader_id.push_str(&self.node_id);

            let request = AppendEntriesRequest {
                term: current_term,
                leader_id,
                prev_log_index,
                prev_log_term,
                entries,
                leader_commit: commit_index,
            };

            let envelope = MessageEnvelope::new(
                &self.node_id,
                peer_id,
                RaftMessage::AppendEntries(request),
            );
            let _ = self.outgoing_tx.send(envelope).await;
        }

        Ok(())
    }

    /// Propose a new command
    pub async fn propose(
        &self,
        command: Command,
        response_tx: mpsc::Sender<ProposeResult>,
    ) -> Result<()> {
        // Only leader can handle proposals
        if *self.role.read().await != NodeRole::Leader {
            let leader_id = self.leader_id.read().await.clone();
            let _ = response_tx.send(ProposeResult::NotLeader { leader_id }).await;
            return Ok(());
        }

        let mut log = self.log.lock().await;
        let term = log.current_term();
        let index = log.append(term, command)?;
        drop(log);

        // Store pending request
        self.pending_requests
            .lock()
            .await
            .insert(index, response_tx);

        // Replicate to followers
        self.send_heartbeats().await?;

        Ok(())
    }

    /// Try to advance commit index (leader only)
    async fn try_advance_commit_index(&self) -> Result<()> {
        if *self.role.read().await != NodeRole::Leader {
            return Ok(());
        }

        let log = self.log.lock().await;
        let current_term = log.current_term();
        let last_log_index = log.last_index();

        // Get current commit index first, then release to avoid holding while iterating
        let current_commit = *self.commit_index.read().await;

        let match_index = self.match_index.read().await;
        let peers = self.peers.read().await;

        let total_nodes = peers.len() + 1;
        let majority = total_nodes / 2 + 1;

        debug!(
            "try_advance_commit: current_commit={}, last_log={}, majority={}, match_index={:?}",
            current_commit, last_log_index, majority, *match_index
        );

        // Find the highest index replicated to majority
        for n in ((current_commit + 1)..=last_log_index).rev() {
            let term_at_n = log.term_at(n);
            if term_at_n != Some(current_term) {
                debug!("Index {} has term {:?}, current is {}, skipping", n, term_at_n, current_term);
                continue;
            }

            let mut count = 1; // Self
            for (_peer_id, &idx) in match_index.iter() {
                if idx >= n {
                    count += 1;
                }
            }

            debug!("Index {}: count={}, majority={}", n, count, majority);

            if count >= majority {
                // Release all locks before modifying commit_index
                drop(match_index);
                drop(peers);
                drop(log);

                info!("Committing index {} (replicated to {} nodes)", n, count);
                *self.commit_index.write().await = n;
                self.apply_committed_entries().await?;
                self.notify_pending_requests(n).await;
                return Ok(());
            }
        }

        Ok(())
    }

    /// Apply committed entries to state machine
    async fn apply_committed_entries(&self) -> Result<()> {
        let commit_index = *self.commit_index.read().await;
        let mut last_applied = self.last_applied.write().await;
        let log = self.log.lock().await;
        let mut state_machine = self.state_machine.lock().await;

        while *last_applied < commit_index {
            *last_applied += 1;
            if let Some(entry) = log.get(*last_applied) {
                state_machine.apply(entry);
                debug!("Applied entry {} to state machine", last_applied);
            }
        }

        Ok(())
    }

    /// Notify pending client requests
    async fn notify_pending_requests(&self, up_to_index: u64) {
        let mut pending = self.pending_requests.lock().await;
        let indices: Vec<u64> = pending
            .keys()
            .filter(|&&idx| idx <= up_to_index)
            .copied()
            .collect();

        for idx in indices {
            if let Some(tx) = pending.remove(&idx) {
                let _ = tx.send(ProposeResult::Success { index: idx }).await;
            }
        }
    }

    /// Add a peer to the cluster
    pub async fn add_peer(&self, id: String, address: String) {
        let info = PeerInfo::new(&id, &address);
        self.peers.write().await.insert(id.clone(), info);

        if *self.role.read().await == NodeRole::Leader {
            let log = self.log.lock().await;
            let last_index = log.last_index();
            self.next_index.write().await.insert(id.clone(), last_index + 1);
            self.match_index.write().await.insert(id, 0);
        }
    }

    /// Remove a peer from the cluster
    pub async fn remove_peer(&self, id: &str) {
        self.peers.write().await.remove(id);
        self.next_index.write().await.remove(id);
        self.match_index.write().await.remove(id);
    }

    /// Get current node status
    pub async fn get_status(&self) -> NodeStatus {
        let log = self.log.lock().await;
        let peers = self.peers.read().await;

        NodeStatus {
            node_id: self.node_id.clone(),
            role: self.role.read().await.clone(),
            current_term: log.current_term(),
            commit_index: *self.commit_index.read().await,
            last_applied: *self.last_applied.read().await,
            leader_id: self.leader_id.read().await.clone(),
            peers: peers.keys().cloned().collect(),
        }
    }

    /// Get time since last heartbeat
    pub async fn time_since_heartbeat(&self) -> Duration {
        self.last_heartbeat.read().await.elapsed()
    }

    /// Get election timeout duration
    pub fn election_timeout(&self) -> Duration {
        use rand::Rng;
        let min = self.config.timing.election_timeout_min_ms;
        let max = self.config.timing.election_timeout_max_ms;
        let timeout_ms = rand::thread_rng().gen_range(min..=max);
        Duration::from_millis(timeout_ms)
    }

    /// Get heartbeat interval
    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_millis(self.config.timing.heartbeat_interval_ms)
    }

    /// Get a value from the state machine
    pub async fn get_value(&self, key: &str) -> Option<Vec<u8>> {
        self.state_machine.lock().await.get(key)
    }

    /// Get recent messages from the state machine
    pub async fn get_messages(&self, limit: u64, _since_index: u64) -> Vec<serde_json::Value> {
        self.state_machine.lock().await.get_messages(limit)
    }
}
