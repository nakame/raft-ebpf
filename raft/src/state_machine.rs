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

    #[test]
    fn test_new_state_machine() {
        let sm = StateMachine::new();
        assert_eq!(sm.last_applied(), 0);
        assert!(sm.keys().is_empty());
    }

    #[test]
    fn test_default_state_machine() {
        let sm = StateMachine::default();
        assert_eq!(sm.last_applied(), 0);
    }

    #[test]
    fn test_apply_put_command() {
        let mut sm = StateMachine::new();
        let entry = LogEntry {
            index: 1,
            term: 1,
            command: Command::Put {
                key: make_key("test_key"),
                value: make_value(b"test_value"),
            },
        };

        let result = sm.apply(&entry);
        assert!(matches!(result, ApplyResult::Ok));
        assert_eq!(sm.last_applied(), 1);
        assert_eq!(sm.get("test_key"), Some(b"test_value".to_vec()));
    }

    #[test]
    fn test_apply_delete_command() {
        let mut sm = StateMachine::new();

        // First put a value
        let put_entry = LogEntry {
            index: 1,
            term: 1,
            command: Command::Put {
                key: make_key("to_delete"),
                value: make_value(b"value"),
            },
        };
        sm.apply(&put_entry);
        assert!(sm.get("to_delete").is_some());

        // Then delete it
        let delete_entry = LogEntry {
            index: 2,
            term: 1,
            command: Command::Delete {
                key: make_key("to_delete"),
            },
        };
        let result = sm.apply(&delete_entry);
        assert!(matches!(result, ApplyResult::Ok));
        assert_eq!(sm.last_applied(), 2);
        assert!(sm.get("to_delete").is_none());
    }

    #[test]
    fn test_apply_noop_command() {
        let mut sm = StateMachine::new();
        let entry = LogEntry {
            index: 1,
            term: 1,
            command: Command::Noop,
        };

        let result = sm.apply(&entry);
        assert!(matches!(result, ApplyResult::Noop));
        assert_eq!(sm.last_applied(), 1);
    }

    #[test]
    fn test_apply_already_applied_entry() {
        let mut sm = StateMachine::new();

        // Apply first entry
        let entry1 = LogEntry {
            index: 1,
            term: 1,
            command: Command::Put {
                key: make_key("key1"),
                value: make_value(b"value1"),
            },
        };
        sm.apply(&entry1);

        // Try to apply an earlier entry
        let entry0 = LogEntry {
            index: 0,
            term: 1,
            command: Command::Put {
                key: make_key("key0"),
                value: make_value(b"should_not_apply"),
            },
        };
        let result = sm.apply(&entry0);
        assert!(matches!(result, ApplyResult::Ok)); // Returns Ok but doesn't modify
        assert!(sm.get("key0").is_none()); // Should not have been applied
        assert_eq!(sm.last_applied(), 1); // Should still be 1
    }

    #[test]
    fn test_get_nonexistent_key() {
        let sm = StateMachine::new();
        assert!(sm.get("nonexistent").is_none());
    }

    #[test]
    fn test_keys_returns_all_keys() {
        let mut sm = StateMachine::new();

        for i in 1..=3 {
            let key = match i {
                1 => "alpha",
                2 => "beta",
                _ => "gamma",
            };
            let entry = LogEntry {
                index: i,
                term: 1,
                command: Command::Put {
                    key: make_key(key),
                    value: make_value(b"value"),
                },
            };
            sm.apply(&entry);
        }

        let keys = sm.keys();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"alpha".to_string()));
        assert!(keys.contains(&"beta".to_string()));
        assert!(keys.contains(&"gamma".to_string()));
    }

    #[test]
    fn test_overwrite_value() {
        let mut sm = StateMachine::new();

        // Put initial value
        let entry1 = LogEntry {
            index: 1,
            term: 1,
            command: Command::Put {
                key: make_key("key"),
                value: make_value(b"initial"),
            },
        };
        sm.apply(&entry1);
        assert_eq!(sm.get("key"), Some(b"initial".to_vec()));

        // Overwrite with new value
        let entry2 = LogEntry {
            index: 2,
            term: 1,
            command: Command::Put {
                key: make_key("key"),
                value: make_value(b"updated"),
            },
        };
        sm.apply(&entry2);
        assert_eq!(sm.get("key"), Some(b"updated".to_vec()));
    }

    #[test]
    fn test_get_messages() {
        let mut sm = StateMachine::new();

        // Add some regular keys
        let entry1 = LogEntry {
            index: 1,
            term: 1,
            command: Command::Put {
                key: make_key("regular_key"),
                value: make_value(b"regular_value"),
            },
        };
        sm.apply(&entry1);

        // Add messages with _msg: prefix
        let msg1 = serde_json::json!({"from": "node-01", "message": "Hello", "timestamp": 1000});
        let entry2 = LogEntry {
            index: 2,
            term: 1,
            command: Command::Put {
                key: make_key("_msg:1000"),
                value: make_value(serde_json::to_vec(&msg1).unwrap().as_slice()),
            },
        };
        sm.apply(&entry2);

        let msg2 = serde_json::json!({"from": "node-02", "message": "World", "timestamp": 2000});
        let entry3 = LogEntry {
            index: 3,
            term: 1,
            command: Command::Put {
                key: make_key("_msg:2000"),
                value: make_value(serde_json::to_vec(&msg2).unwrap().as_slice()),
            },
        };
        sm.apply(&entry3);

        let messages = sm.get_messages(10);
        assert_eq!(messages.len(), 2);
        // Should be sorted newest first
        assert_eq!(messages[0]["timestamp"], 2000);
        assert_eq!(messages[1]["timestamp"], 1000);
    }

    #[test]
    fn test_get_messages_with_limit() {
        let mut sm = StateMachine::new();

        // Add 5 messages
        for i in 1..=5 {
            let msg = serde_json::json!({"message": i, "timestamp": i * 1000});
            let ts_key = format!("_msg:{}", i * 1000);
            let entry = LogEntry {
                index: i,
                term: 1,
                command: Command::Put {
                    key: make_key(&ts_key),
                    value: make_value(serde_json::to_vec(&msg).unwrap().as_slice()),
                },
            };
            sm.apply(&entry);
        }

        // Get only 2 messages
        let messages = sm.get_messages(2);
        assert_eq!(messages.len(), 2);
        // Should have the newest ones (5000, 4000)
        assert_eq!(messages[0]["timestamp"], 5000);
        assert_eq!(messages[1]["timestamp"], 4000);
    }

    #[test]
    fn test_snapshot_and_restore() {
        let mut sm = StateMachine::new();

        // Add some data
        let entry1 = LogEntry {
            index: 1,
            term: 1,
            command: Command::Put {
                key: make_key("key1"),
                value: make_value(b"value1"),
            },
        };
        sm.apply(&entry1);

        let entry2 = LogEntry {
            index: 2,
            term: 1,
            command: Command::Put {
                key: make_key("key2"),
                value: make_value(b"value2"),
            },
        };
        sm.apply(&entry2);

        // Take snapshot
        let snapshot = sm.snapshot();
        assert!(!snapshot.is_empty());

        // Create new state machine and restore
        let mut sm2 = StateMachine::new();
        sm2.restore(&snapshot, 2).expect("restore should succeed");

        assert_eq!(sm2.last_applied(), 2);
        assert_eq!(sm2.get("key1"), Some(b"value1".to_vec()));
        assert_eq!(sm2.get("key2"), Some(b"value2".to_vec()));
    }

    #[test]
    fn test_delete_nonexistent_key() {
        let mut sm = StateMachine::new();
        let entry = LogEntry {
            index: 1,
            term: 1,
            command: Command::Delete {
                key: make_key("nonexistent"),
            },
        };
        // Should not panic or error
        let result = sm.apply(&entry);
        assert!(matches!(result, ApplyResult::Ok));
    }
}
