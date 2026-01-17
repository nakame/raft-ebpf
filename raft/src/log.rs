//! Persistent log storage for RAFT
//!
//! Stores log entries and persistent state to disk.

use anyhow::Result;
use raft_common::{Command, LogEntry, PersistentState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// Persistent log storage
pub struct RaftLog {
    /// Path to the data directory
    data_dir: PathBuf,
    /// In-memory log entries (index -> entry)
    entries: HashMap<u64, LogEntry>,
    /// Persistent state
    persistent_state: PersistentState,
    /// Last log index
    last_index: u64,
    /// Log file handle
    log_file: Option<File>,
}

/// Serializable log entry for disk storage
#[derive(Serialize, Deserialize)]
struct DiskLogEntry {
    index: u64,
    term: u64,
    command_type: String,
    key: Option<String>,
    value: Option<Vec<u8>>,
}

impl RaftLog {
    /// Create a new log storage
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&data_dir)?;

        let mut log = Self {
            data_dir,
            entries: HashMap::new(),
            persistent_state: PersistentState::default(),
            last_index: 0,
            log_file: None,
        };

        log.load_from_disk()?;
        Ok(log)
    }

    /// Load persistent state and log entries from disk
    fn load_from_disk(&mut self) -> Result<()> {
        // Load persistent state
        let state_path = self.data_dir.join("state.json");
        if state_path.exists() {
            let content = fs::read_to_string(&state_path)?;
            self.persistent_state = serde_json::from_str(&content)?;
        }

        // Load log entries
        let log_path = self.data_dir.join("log.jsonl");
        if log_path.exists() {
            let file = File::open(&log_path)?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line?;
                if line.is_empty() {
                    continue;
                }
                let disk_entry: DiskLogEntry = serde_json::from_str(&line)?;
                let entry = self.disk_entry_to_log_entry(disk_entry);
                self.last_index = self.last_index.max(entry.index);
                self.entries.insert(entry.index, entry);
            }
        }

        // Open log file for appending
        let log_path = self.data_dir.join("log.jsonl");
        self.log_file = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)?,
        );

        Ok(())
    }

    fn disk_entry_to_log_entry(&self, disk: DiskLogEntry) -> LogEntry {
        let command = match disk.command_type.as_str() {
            "Put" => {
                let key = disk.key.unwrap_or_default();
                let value = disk.value.unwrap_or_default();
                let mut key_str = heapless::String::new();
                let _ = key_str.push_str(&key);
                let mut value_vec = heapless::Vec::new();
                for b in value {
                    let _ = value_vec.push(b);
                }
                Command::Put {
                    key: key_str,
                    value: value_vec,
                }
            }
            "Delete" => {
                let key = disk.key.unwrap_or_default();
                let mut key_str = heapless::String::new();
                let _ = key_str.push_str(&key);
                Command::Delete { key: key_str }
            }
            _ => Command::Noop,
        };

        LogEntry {
            index: disk.index,
            term: disk.term,
            command,
        }
    }

    fn entry_to_disk(entry: &LogEntry) -> DiskLogEntry {
        let (command_type, key, value) = match &entry.command {
            Command::Put { key, value } => {
                ("Put".to_string(), Some(key.as_str().to_string()), Some(value.to_vec()))
            }
            Command::Delete { key } => ("Delete".to_string(), Some(key.as_str().to_string()), None),
            Command::Noop => ("Noop".to_string(), None, None),
        };

        DiskLogEntry {
            index: entry.index,
            term: entry.term,
            command_type,
            key,
            value,
        }
    }

    /// Save persistent state to disk
    pub fn save_state(&self) -> Result<()> {
        let state_path = self.data_dir.join("state.json");
        let content = serde_json::to_string_pretty(&self.persistent_state)?;
        fs::write(state_path, content)?;
        Ok(())
    }

    /// Get current term
    pub fn current_term(&self) -> u64 {
        self.persistent_state.current_term
    }

    /// Set current term
    pub fn set_current_term(&mut self, term: u64) -> Result<()> {
        self.persistent_state.current_term = term;
        self.persistent_state.voted_for = None;
        self.save_state()
    }

    /// Get voted_for
    pub fn voted_for(&self) -> Option<String> {
        self.persistent_state
            .voted_for
            .as_ref()
            .map(|s| s.as_str().to_string())
    }

    /// Set voted_for
    pub fn set_voted_for(&mut self, candidate_id: Option<&str>) -> Result<()> {
        self.persistent_state.voted_for = candidate_id.map(|s| {
            let mut hs = heapless::String::new();
            let _ = hs.push_str(s);
            hs
        });
        self.save_state()
    }

    /// Get last log index
    pub fn last_index(&self) -> u64 {
        self.last_index
    }

    /// Get last log term
    pub fn last_term(&self) -> u64 {
        self.entries
            .get(&self.last_index)
            .map(|e| e.term)
            .unwrap_or(0)
    }

    /// Get entry at index
    pub fn get(&self, index: u64) -> Option<&LogEntry> {
        self.entries.get(&index)
    }

    /// Get term of entry at index
    pub fn term_at(&self, index: u64) -> Option<u64> {
        self.entries.get(&index).map(|e| e.term)
    }

    /// Append a new entry
    pub fn append(&mut self, term: u64, command: Command) -> Result<u64> {
        let index = self.last_index + 1;
        let entry = LogEntry {
            index,
            term,
            command,
        };

        // Convert to disk format before borrowing file
        let disk_entry = Self::entry_to_disk(&entry);

        // Write to disk
        if let Some(ref mut file) = self.log_file {
            let line = serde_json::to_string(&disk_entry)?;
            writeln!(file, "{}", line)?;
            file.flush()?;
        }

        self.entries.insert(index, entry);
        self.last_index = index;

        Ok(index)
    }

    /// Append entries from leader
    pub fn append_entries(&mut self, entries: Vec<LogEntry>) -> Result<()> {
        for entry in entries {
            // Convert to disk format before borrowing file
            let disk_entry = Self::entry_to_disk(&entry);

            // Write to disk
            if let Some(ref mut file) = self.log_file {
                let line = serde_json::to_string(&disk_entry)?;
                writeln!(file, "{}", line)?;
            }

            self.last_index = self.last_index.max(entry.index);
            self.entries.insert(entry.index, entry);
        }

        if let Some(ref mut file) = self.log_file {
            file.flush()?;
        }

        Ok(())
    }

    /// Delete entries from index onwards (for conflict resolution)
    pub fn truncate_from(&mut self, from_index: u64) -> Result<()> {
        // Remove entries from memory
        self.entries.retain(|&idx, _| idx < from_index);
        self.last_index = from_index.saturating_sub(1);

        // Rewrite log file
        let log_path = self.data_dir.join("log.jsonl");
        let temp_path = self.data_dir.join("log.jsonl.tmp");

        {
            let mut temp_file = File::create(&temp_path)?;
            for idx in 1..from_index {
                if let Some(entry) = self.entries.get(&idx) {
                    let disk_entry = Self::entry_to_disk(entry);
                    let line = serde_json::to_string(&disk_entry)?;
                    writeln!(temp_file, "{}", line)?;
                }
            }
            temp_file.flush()?;
        }

        fs::rename(&temp_path, &log_path)?;

        // Reopen log file
        self.log_file = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)?,
        );

        Ok(())
    }

    /// Get entries from start_index to end_index (inclusive)
    pub fn get_entries(&self, start_index: u64, end_index: u64) -> Vec<LogEntry> {
        (start_index..=end_index)
            .filter_map(|idx| self.entries.get(&idx).cloned())
            .collect()
    }

    /// Check if log contains entry at index with given term
    pub fn has_entry(&self, index: u64, term: u64) -> bool {
        self.entries
            .get(&index)
            .map(|e| e.term == term)
            .unwrap_or(index == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
    fn test_new_raft_log() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        assert_eq!(log.current_term(), 0);
        assert_eq!(log.last_index(), 0);
        assert_eq!(log.last_term(), 0);
        assert!(log.voted_for().is_none());
    }

    #[test]
    fn test_set_current_term() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let mut log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        log.set_current_term(5).expect("set term");
        assert_eq!(log.current_term(), 5);

        // Setting term should clear voted_for
        log.set_voted_for(Some("candidate-01")).expect("vote");
        assert!(log.voted_for().is_some());

        log.set_current_term(6).expect("set term");
        assert_eq!(log.current_term(), 6);
        assert!(log.voted_for().is_none()); // Should be cleared
    }

    #[test]
    fn test_voted_for() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let mut log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        assert!(log.voted_for().is_none());

        log.set_voted_for(Some("node-02")).expect("vote");
        assert_eq!(log.voted_for(), Some("node-02".to_string()));

        log.set_voted_for(None).expect("clear vote");
        assert!(log.voted_for().is_none());
    }

    #[test]
    fn test_append_entry() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let mut log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        let index = log.append(1, Command::Noop).expect("append");
        assert_eq!(index, 1);
        assert_eq!(log.last_index(), 1);
        assert_eq!(log.last_term(), 1);

        let entry = log.get(1).expect("get entry");
        assert_eq!(entry.index, 1);
        assert_eq!(entry.term, 1);
    }

    #[test]
    fn test_append_multiple_entries() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let mut log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        log.append(1, Command::Noop).expect("append 1");
        log.append(1, Command::Put {
            key: make_key("key1"),
            value: make_value(b"value1"),
        }).expect("append 2");
        log.append(2, Command::Delete {
            key: make_key("key1"),
        }).expect("append 3");

        assert_eq!(log.last_index(), 3);
        assert_eq!(log.last_term(), 2);

        assert_eq!(log.term_at(1), Some(1));
        assert_eq!(log.term_at(2), Some(1));
        assert_eq!(log.term_at(3), Some(2));
    }

    #[test]
    fn test_get_entries_range() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let mut log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        for i in 1..=5 {
            log.append(1, Command::Noop).expect("append");
        }

        let entries = log.get_entries(2, 4);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].index, 2);
        assert_eq!(entries[1].index, 3);
        assert_eq!(entries[2].index, 4);
    }

    #[test]
    fn test_has_entry() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let mut log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        log.append(1, Command::Noop).expect("append");
        log.append(2, Command::Noop).expect("append");

        assert!(log.has_entry(0, 0)); // Index 0 always matches
        assert!(log.has_entry(1, 1)); // Correct term
        assert!(!log.has_entry(1, 2)); // Wrong term
        assert!(log.has_entry(2, 2)); // Correct term
        assert!(!log.has_entry(3, 1)); // Non-existent index
    }

    #[test]
    fn test_truncate_from() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let mut log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        for i in 1..=5 {
            log.append(1, Command::Noop).expect("append");
        }
        assert_eq!(log.last_index(), 5);

        log.truncate_from(3).expect("truncate");
        assert_eq!(log.last_index(), 2);
        assert!(log.get(1).is_some());
        assert!(log.get(2).is_some());
        assert!(log.get(3).is_none());
        assert!(log.get(4).is_none());
        assert!(log.get(5).is_none());
    }

    #[test]
    fn test_append_entries_from_leader() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let mut log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        let entries = vec![
            LogEntry {
                index: 1,
                term: 1,
                command: Command::Noop,
            },
            LogEntry {
                index: 2,
                term: 1,
                command: Command::Put {
                    key: make_key("k"),
                    value: make_value(b"v"),
                },
            },
        ];

        log.append_entries(entries).expect("append entries");
        assert_eq!(log.last_index(), 2);
        assert!(log.get(1).is_some());
        assert!(log.get(2).is_some());
    }

    #[test]
    fn test_persistence_across_restarts() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let data_path = temp_dir.path().to_path_buf();

        // Create log, add entries, and drop it
        {
            let mut log = RaftLog::new(data_path.clone()).expect("create log");
            log.set_current_term(5).expect("set term");
            log.set_voted_for(Some("node-01")).expect("vote");
            log.append(5, Command::Put {
                key: make_key("persistent"),
                value: make_value(b"data"),
            }).expect("append");
        }

        // Reopen log and verify state persisted
        {
            let log = RaftLog::new(data_path).expect("reopen log");
            assert_eq!(log.current_term(), 5);
            assert_eq!(log.voted_for(), Some("node-01".to_string()));
            assert_eq!(log.last_index(), 1);
            assert_eq!(log.last_term(), 5);

            let entry = log.get(1).expect("get entry");
            if let Command::Put { key, value } = &entry.command {
                assert_eq!(key.as_str(), "persistent");
                assert_eq!(value.as_slice(), b"data");
            } else {
                panic!("Expected Put command");
            }
        }
    }

    #[test]
    fn test_term_at_nonexistent_index() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        assert!(log.term_at(999).is_none());
    }

    #[test]
    fn test_get_nonexistent_entry() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        assert!(log.get(999).is_none());
    }

    #[test]
    fn test_empty_get_entries_range() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let log = RaftLog::new(temp_dir.path().to_path_buf()).expect("create log");

        let entries = log.get_entries(1, 10);
        assert!(entries.is_empty());
    }
}
