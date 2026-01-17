//! eBPF map definitions for kernel-userspace communication
//!
//! These structures are used to pass data between the eBPF kernel program
//! and the userspace RAFT daemon via BPF maps.

/// Metadata about a peer, stored in the PEER_STATE BPF map
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PeerMetadata {
    /// Kernel timestamp (nanoseconds) when last packet was seen
    pub last_seen_ns: u64,
    /// Number of packets received from this peer
    pub packet_count: u64,
    /// Last observed TCP sequence number
    pub last_seq: u32,
    /// Padding for alignment
    pub _padding: u32,
}

#[cfg(feature = "aya")]
unsafe impl aya::Pod for PeerMetadata {}

/// Event sent from eBPF to userspace via ring buffer
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RaftPacketEvent {
    /// Source IP address (network byte order)
    pub src_ip: u32,
    /// Destination IP address (network byte order)
    pub dst_ip: u32,
    /// Source port (host byte order)
    pub src_port: u16,
    /// Destination port (host byte order)
    pub dst_port: u16,
    /// Kernel timestamp (nanoseconds)
    pub timestamp_ns: u64,
    /// TCP sequence number
    pub seq: u32,
    /// TCP acknowledgment number
    pub ack: u32,
    /// TCP flags
    pub flags: u8,
    /// Message type detected (0=unknown, 1=AppendEntries, 2=RequestVote, 3=Response)
    pub msg_type: u8,
    /// Padding for alignment
    pub _padding: [u8; 6],
}

#[cfg(feature = "aya")]
unsafe impl aya::Pod for RaftPacketEvent {}

/// Statistics tracked by the eBPF program
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct EbpfStats {
    /// Total packets processed
    pub packets_total: u64,
    /// RAFT packets detected
    pub raft_packets: u64,
    /// Packets dropped due to errors
    pub errors: u64,
}

#[cfg(feature = "aya")]
unsafe impl aya::Pod for EbpfStats {}

/// Map names used in the eBPF program
pub mod map_names {
    /// HashMap storing peer metadata keyed by IP address
    pub const PEER_STATE: &str = "PEER_STATE";
    /// Ring buffer for packet events
    pub const RAFT_EVENTS: &str = "RAFT_EVENTS";
    /// Array for statistics
    pub const STATS: &str = "STATS";
}
