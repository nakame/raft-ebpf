# RAFT Consensus in eBPF with Rust

A distributed RAFT consensus implementation using Rust and the Aya eBPF library.

## Overview

This project implements the RAFT consensus algorithm with:
- **Rust** for all code (userspace and kernel)
- **Aya** library for eBPF programs
- **TCP** transport for node communication
- **Target**: ARM64 (Radxa Rock 5B) and x86_64

## Quick Start

### 1. Start the Cluster

Open 3 terminals and start each node:

```bash
# Terminal 1 - node-01
limactl shell raft-node-01
cd /Users/jase/raft
./target/release/raft --config nodes/node-01/raft.toml

# Terminal 2 - node-02
limactl shell raft-node-02
cd /Users/jase/raft
./target/release/raft --config nodes/node-02/raft.toml

# Terminal 3 - node-03
limactl shell raft-node-03
cd /Users/jase/raft
./target/release/raft --config nodes/node-03/raft.toml
```

### 2. Check Cluster Status

```bash
$ ./target/release/raft-cli --node node-02 status

RAFT Cluster Status
==================
Node ID:      node-02
Role:         Leader
Current Term: 1783
Commit Index: 4
Last Applied: 4
Leader:       node-02
Peers:        ["node-01", "node-03"]
```

```bash
$ ./target/release/raft-cli --node node-01 status

RAFT Cluster Status
==================
Node ID:      node-01
Role:         Follower
Current Term: 1783
Commit Index: 4
Last Applied: 4
Leader:       node-02
Peers:        ["node-02", "node-03"]
```

### 3. Send a Message (from the Leader)

Messages must be sent from the leader node. The CLI will tell you which node is the leader:

```bash
$ ./target/release/raft-cli --node node-02 send "Hello from the RAFT cluster!"

✓ Message sent: "Hello from the RAFT cluster!"
  Replicated at log index 5
```

If you try to send from a follower:
```bash
$ ./target/release/raft-cli --node node-01 send "Test message"

✗ Failed to send: Not leader - send to leader node
  Hint: Send from leader node: node-02
```

### 4. View Messages (from Any Node)

Messages are replicated to all nodes via RAFT consensus:

```bash
$ ./target/release/raft-cli --node node-01 messages --limit 5

Messages (last applied index: 5)
==================================================
[02:58:00] node-02: Hello from the RAFT cluster!
[02:55:39] node-02: Message 3 from leader
[02:55:37] node-02: Message 2 from leader
[02:55:25] node-02: Hello from the leader!
[02:42:11] node-01: Hello from node-01!
```

```bash
$ ./target/release/raft-cli --node node-03 messages --limit 5

Messages (last applied index: 5)
==================================================
[02:58:00] node-02: Hello from the RAFT cluster!
[02:55:39] node-02: Message 3 from leader
[02:55:37] node-02: Message 2 from leader
[02:55:25] node-02: Hello from the leader!
[02:42:11] node-01: Hello from node-01!
```

### 5. Watch Messages in Real-Time

Open a watcher on any node to see new messages as they arrive:

```bash
$ ./target/release/raft-cli --node node-01 watch

Watching for messages on node-01 (Ctrl+C to stop)...
==================================================
[02:58:00] node-02: Hello from the RAFT cluster!
[03:01:15] node-02: New message just sent!
[03:01:20] node-02: Another message!
```

## CLI Reference

### Commands

| Command | Description |
|---------|-------------|
| `status` | Show cluster status (role, term, leader, peers) |
| `send "msg"` | Send a message (leader only) |
| `messages` | List recent messages |
| `watch` | Watch for new messages in real-time |
| `propose put <key> <value>` | Store a key-value pair |
| `propose get <key>` | Retrieve a value |
| `propose delete <key>` | Delete a key |
| `join <id> <addr>` | Add a peer to the cluster |
| `leave <id>` | Remove a peer from the cluster |

### Options

| Option | Description |
|--------|-------------|
| `--node <id>` | Node to connect to (default: node-0) |
| `--limit <n>` | Number of messages to show (default: 10) |
| `--interval <ms>` | Watch poll interval in ms (default: 500) |

### Examples

```bash
# Check which node is the leader
raft-cli --node node-01 status

# Send a message from the leader
raft-cli --node node-02 send "Hello world!"

# View last 20 messages
raft-cli --node node-01 messages --limit 20

# Watch with 1 second polling
raft-cli --node node-03 watch --interval 1000

# Store a key-value pair
raft-cli --node node-02 propose put mykey "my value"

# Retrieve a value
raft-cli --node node-01 propose get mykey
```

## How It Works

### Message Flow

1. **Client sends message** to the leader via `raft-cli send`
2. **Leader appends** the message to its log
3. **Leader replicates** to followers via `AppendEntries` RPC
4. **Followers acknowledge** with `AppendEntriesResponse`
5. **Leader commits** once a majority has replicated
6. **All nodes apply** the message to their state machine
7. **Clients can read** the message from any node

### Consensus Guarantees

- Messages are **durably stored** before acknowledgment
- Messages appear in the **same order** on all nodes
- Messages are **committed** only after majority replication
- **Leader election** happens automatically if the leader fails

## Project Structure

```
raft/
├── Cargo.toml                    # Workspace root
├── Cross.toml                    # ARM64 cross-compilation
├── LIMA.md                       # Lima VM setup guide
├── raft-common/                  # Shared types (no_std compatible)
│   └── src/
│       ├── lib.rs
│       ├── protocol.rs           # RAFT messages
│       ├── state.rs              # Node state enums
│       └── maps.rs               # eBPF map definitions
├── raft-ebpf/                    # Kernel eBPF program
│   └── src/
│       └── main.rs               # TC classifier
├── raft/                         # Userspace daemon
│   └── src/
│       ├── main.rs               # Entry point
│       ├── node.rs               # Core RAFT logic
│       ├── state_machine.rs      # Command application
│       ├── log.rs                # Persistent log storage
│       ├── network.rs            # TCP RPC server
│       ├── ebpf.rs               # eBPF loader (Linux only)
│       ├── ipc.rs                # Unix socket IPC
│       └── config.rs             # Configuration
├── raft-cli/                     # CLI tool
│   └── src/
│       ├── main.rs
│       └── commands/
│           ├── mod.rs
│           └── client.rs         # IPC client
└── nodes/                        # Test cluster configs
    ├── node-01/
    │   ├── lima.yaml             # Lima VM config (Debian 12)
    │   └── raft.toml             # RAFT config
    ├── node-02/
    └── node-03/
```

## Building

### On Linux (native or cross-compile)

```bash
# Build all crates
cargo build --release

# Build eBPF program (requires bpf-linker)
cargo +nightly build --package raft-ebpf --release \
  --target=bpfel-unknown-none \
  -Z build-std=core
```

### On macOS (for testing)

The eBPF program only compiles on Linux. On macOS, the daemon runs without eBPF support:

```bash
cargo build --release
```

### Cross-compile for ARM64

```bash
cross build --release --target aarch64-unknown-linux-gnu
```

## Configuration

Create a `raft.toml` configuration file:

```toml
[server]
node_id = "node-01"
listen_addr = "0.0.0.0:5555"
data_dir = "/tmp/raft/node-01"
log_level = "debug"

[timing]
heartbeat_interval_ms = 50
election_timeout_min_ms = 150
election_timeout_max_ms = 300

[ebpf]
enabled = false
interface = "eth0"

# Peers on shared vmnet network
[[peers]]
id = "node-02"
address = "192.168.105.12:5555"

[[peers]]
id = "node-03"
address = "192.168.105.13:5555"
```

## Testing with Lima VMs

See [LIMA.md](LIMA.md) for detailed setup instructions.

### Quick Setup

```bash
# Create VMs (Debian 12)
cd ~/raft/nodes/node-01 && limactl create --name=raft-node-01 lima.yaml
cd ~/raft/nodes/node-02 && limactl create --name=raft-node-02 lima.yaml
cd ~/raft/nodes/node-03 && limactl create --name=raft-node-03 lima.yaml

# Start VMs
limactl start raft-node-01
limactl start raft-node-02
limactl start raft-node-03

# Get VM IPs (on shared vmnet)
limactl shell raft-node-01 -- hostname -I  # 192.168.105.11
limactl shell raft-node-02 -- hostname -I  # 192.168.105.12
limactl shell raft-node-03 -- hostname -I  # 192.168.105.13
```

### Build Inside VM

```bash
limactl shell raft-node-01
cd /Users/jase/raft
cargo build --release
```

### Run as Background Processes

```bash
# Start nodes in background
limactl shell raft-node-01 -- bash -c 'cd /Users/jase/raft && nohup ./target/release/raft --config nodes/node-01/raft.toml > /tmp/raft-node-01.log 2>&1 &'
limactl shell raft-node-02 -- bash -c 'cd /Users/jase/raft && nohup ./target/release/raft --config nodes/node-02/raft.toml > /tmp/raft-node-02.log 2>&1 &'
limactl shell raft-node-03 -- bash -c 'cd /Users/jase/raft && nohup ./target/release/raft --config nodes/node-03/raft.toml > /tmp/raft-node-03.log 2>&1 &'

# View logs
limactl shell raft-node-01 -- tail -f /tmp/raft-node-01.log
```

## Debugging

### Log Levels

Control verbosity via `RUST_LOG` environment variable:

```bash
# Show only INFO and above
RUST_LOG=info ./target/release/raft --config raft.toml

# Show DEBUG messages (includes all RAFT protocol messages)
RUST_LOG=debug ./target/release/raft --config raft.toml

# Show only network-related DEBUG messages
RUST_LOG=raft::network=debug ./target/release/raft --config raft.toml
```

### Sample Debug Output

**Leader sending heartbeats:**
```
DEBUG raft::network: Received message from node-02: AppendEntriesResponse { term: 1783, success: true, last_log_index: 5 }
DEBUG raft::network: Received message from node-03: AppendEntriesResponse { term: 1783, success: true, last_log_index: 5 }
```

**Follower receiving entries:**
```
DEBUG raft::network: Accepted connection from 192.168.105.12:50090
DEBUG raft::network: Received message from node-02: AppendEntries { term: 1783, entries: [...] }
```

### Message Types

| Message | Description |
|---------|-------------|
| `RequestVote` | Candidate requesting votes during election |
| `RequestVoteResponse` | Follower's vote response |
| `AppendEntries` | Leader sending log entries (or heartbeat if empty) |
| `AppendEntriesResponse` | Follower acknowledging entries |

## Architecture

```
┌─────────────────────────────────────────────┐
│           Client Applications               │
│      (read replicated state via IPC)        │
└─────────────────┬───────────────────────────┘
                  │ Unix Socket (/tmp/raft-<node>.sock)
                  ▼
┌─────────────────────────────────────────────┐
│            RAFT Daemon (Userspace)          │
│  ┌────────────────────────────────────────┐ │
│  │ State Machine: HashMap<String, Vec<u8>>│ │
│  └────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────┐ │
│  │ RAFT Core: Log, Term, VotedFor, etc.   │ │
│  └────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────┐ │
│  │ TCP Server (port 5555)                 │ │
│  └────────────────────────────────────────┘ │
└───────────────┬─────────────────────────────┘
                │
    ┌───────────┴───────────┐
    │                       │
    ▼                       ▼
┌──────────┐       ┌──────────────────┐
│ Other    │       │ eBPF TC Classifier│
│ Nodes    │       │ (kernel space)    │
│ (TCP)    │       │ - Filter RAFT pkts│
└──────────┘       │ - Update maps     │
                   │ - Ring buf events │
                   └──────────────────┘
```

## RAFT Protocol

### Leader Election
- Election timeout: 150-300ms (randomized)
- Heartbeat interval: 50ms
- RequestVote RPC with term, lastLogIndex, lastLogTerm
- Majority vote wins

### Log Replication
- AppendEntries RPC with entries, prevLogIndex, prevLogTerm
- Leader maintains nextIndex/matchIndex per follower
- Commit when replicated to majority

### State Transitions
```
Follower ─(timeout)─> Candidate ─(win votes)─> Leader
    ^                     │                      │
    │                     │                      │
    └─────(higher term)───┴──────────────────────┘
```

## License

MIT
