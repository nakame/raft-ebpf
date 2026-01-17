//! State machine for applying committed log entries

use raft_common::{Command, LogEntry};
use std::collections::HashMap;

/// The replicated state machine
///
/// This is a simple key-value store that applies committed log entries.
pub struct StateMachine {
    /// The actual key-value data
    data: HashMap<String, Vec<u8>>,
    /// Index of the last applied entry
    last_applied: u64,
}

/// Result of applying a command
#[derive(Debug, Clone)]
pub enum ApplyResult {
    /// Successfully stored/deleted
    Ok,
    /// Value retrieved for a key
    Value(Option<Vec<u8>>),
    /// No-op applied
    Noop,
}

impl StateMachine {
    /// Create a new empty state machine
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            last_applied: 0,
        }
    }

    /// Get the last applied index
    pub fn last_applied(&self) -> u64 {
        self.last_applied
    }

    /// Apply a log entry to the state machine
    pub fn apply(&mut self, entry: &LogEntry) -> ApplyResult {
        if entry.index <= self.last_applied {
            // Already applied
            return ApplyResult::Ok;
        }

        let result = match &entry.command {
            Command::Put { key, value } => {
                self.data
                    .insert(key.as_str().to_string(), value.to_vec());
                ApplyResult::Ok
            }
            Command::Delete { key } => {
                self.data.remove(key.as_str());
                ApplyResult::Ok
            }
            Command::Noop => ApplyResult::Noop,
        };

        self.last_applied = entry.index;
        result
    }

    /// Get a value from the state machine
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.data.get(key).cloned()
    }

    /// Get all keys in the state machine
    pub fn keys(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }

    /// Get recent messages (keys starting with _msg:)
    pub fn get_messages(&self, limit: u64) -> Vec<serde_json::Value> {
        let mut messages: Vec<(u128, serde_json::Value)> = self
            .data
            .iter()
            .filter(|(k, _)| k.starts_with("_msg:"))
            .filter_map(|(k, v)| {
                let timestamp: u128 = k.strip_prefix("_msg:")?.parse().ok()?;
                let msg: serde_json::Value = serde_json::from_slice(v).ok()?;
                Some((timestamp, msg))
            })
            .collect();

        // Sort by timestamp descending (newest first)
        messages.sort_by(|a, b| b.0.cmp(&a.0));

        // Take the limit and return just the message values
        messages
            .into_iter()
            .take(limit as usize)
            .map(|(_, msg)| msg)
            .collect()
    }

    /// Create a snapshot of the state machine
    pub fn snapshot(&self) -> Vec<u8> {
        bincode::serialize(&self.data).unwrap_or_default()
    }

    /// Restore from a snapshot
    pub fn restore(&mut self, snapshot: &[u8], last_index: u64) -> anyhow::Result<()> {
        self.data = bincode::deserialize(snapshot)?;
        self.last_applied = last_index;
        Ok(())
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}
