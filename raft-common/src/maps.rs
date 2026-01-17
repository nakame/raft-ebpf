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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_metadata_default() {
        let meta = PeerMetadata::default();
        assert_eq!(meta.last_seen_ns, 0);
        assert_eq!(meta.packet_count, 0);
        assert_eq!(meta.last_seq, 0);
        assert_eq!(meta._padding, 0);
    }

    #[test]
    fn test_peer_metadata_creation() {
        let meta = PeerMetadata {
            last_seen_ns: 1_000_000_000,
            packet_count: 42,
            last_seq: 12345,
            _padding: 0,
        };
        assert_eq!(meta.last_seen_ns, 1_000_000_000);
        assert_eq!(meta.packet_count, 42);
        assert_eq!(meta.last_seq, 12345);
    }

    #[test]
    fn test_peer_metadata_copy() {
        let meta1 = PeerMetadata {
            last_seen_ns: 100,
            packet_count: 10,
            last_seq: 1000,
            _padding: 0,
        };
        let meta2 = meta1;
        assert_eq!(meta1.last_seen_ns, meta2.last_seen_ns);
        assert_eq!(meta1.packet_count, meta2.packet_count);
        assert_eq!(meta1.last_seq, meta2.last_seq);
    }

    #[test]
    fn test_raft_packet_event_default() {
        let event = RaftPacketEvent::default();
        assert_eq!(event.src_ip, 0);
        assert_eq!(event.dst_ip, 0);
        assert_eq!(event.src_port, 0);
        assert_eq!(event.dst_port, 0);
        assert_eq!(event.timestamp_ns, 0);
        assert_eq!(event.seq, 0);
        assert_eq!(event.ack, 0);
        assert_eq!(event.flags, 0);
        assert_eq!(event.msg_type, 0);
    }

    #[test]
    fn test_raft_packet_event_creation() {
        let event = RaftPacketEvent {
            src_ip: 0xC0A80A01, // 192.168.10.1
            dst_ip: 0xC0A80A02, // 192.168.10.2
            src_port: 45678,
            dst_port: 5555,
            timestamp_ns: 123456789,
            seq: 1000,
            ack: 999,
            flags: 0x18, // PSH+ACK
            msg_type: 1, // AppendEntries
            _padding: [0; 6],
        };
        assert_eq!(event.src_ip, 0xC0A80A01);
        assert_eq!(event.dst_ip, 0xC0A80A02);
        assert_eq!(event.src_port, 45678);
        assert_eq!(event.dst_port, 5555);
        assert_eq!(event.msg_type, 1);
    }

    #[test]
    fn test_raft_packet_event_copy() {
        let event1 = RaftPacketEvent {
            src_ip: 1,
            dst_ip: 2,
            src_port: 3,
            dst_port: 4,
            timestamp_ns: 5,
            seq: 6,
            ack: 7,
            flags: 8,
            msg_type: 9,
            _padding: [0; 6],
        };
        let event2 = event1;
        assert_eq!(event1.src_ip, event2.src_ip);
        assert_eq!(event1.msg_type, event2.msg_type);
    }

    #[test]
    fn test_ebpf_stats_default() {
        let stats = EbpfStats::default();
        assert_eq!(stats.packets_total, 0);
        assert_eq!(stats.raft_packets, 0);
        assert_eq!(stats.errors, 0);
    }

    #[test]
    fn test_ebpf_stats_creation() {
        let stats = EbpfStats {
            packets_total: 1000,
            raft_packets: 500,
            errors: 10,
        };
        assert_eq!(stats.packets_total, 1000);
        assert_eq!(stats.raft_packets, 500);
        assert_eq!(stats.errors, 10);
    }

    #[test]
    fn test_ebpf_stats_copy() {
        let stats1 = EbpfStats {
            packets_total: 100,
            raft_packets: 50,
            errors: 5,
        };
        let stats2 = stats1;
        assert_eq!(stats1.packets_total, stats2.packets_total);
        assert_eq!(stats1.raft_packets, stats2.raft_packets);
        assert_eq!(stats1.errors, stats2.errors);
    }

    #[test]
    fn test_map_names_constants() {
        assert_eq!(map_names::PEER_STATE, "PEER_STATE");
        assert_eq!(map_names::RAFT_EVENTS, "RAFT_EVENTS");
        assert_eq!(map_names::STATS, "STATS");
    }

    #[test]
    fn test_peer_metadata_size() {
        // Verify struct is properly aligned for BPF maps
        assert_eq!(core::mem::size_of::<PeerMetadata>(), 24);
    }

    #[test]
    fn test_raft_packet_event_size() {
        // Verify struct is properly aligned for BPF ring buffer
        // 4+4+2+2+8+4+4+1+1+6 = 36, padded to 40 for alignment
        assert_eq!(core::mem::size_of::<RaftPacketEvent>(), 40);
    }

    #[test]
    fn test_ebpf_stats_size() {
        assert_eq!(core::mem::size_of::<EbpfStats>(), 24);
    }
}
