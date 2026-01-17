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
