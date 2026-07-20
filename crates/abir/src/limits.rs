#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidationLimits {
    pub max_recordings: usize,
    pub max_streams: usize,
    pub max_atoms: usize,
    pub max_catalog_records: usize,
    pub max_relationships: usize,
    pub max_governance_records: usize,
    pub max_channels: usize,
    pub max_rank: usize,
    pub max_nesting_depth: usize,
    /// Maximum target-independent semantic metadata budget. This is not a
    /// retained-memory or allocator-capacity ceiling.
    pub max_metadata_bytes: usize,
    pub max_logical_payload_bytes: u64,
}

impl Default for ValidationLimits {
    fn default() -> Self {
        Self {
            max_recordings: 1_000_000,
            max_streams: 4_000_000,
            max_atoms: 64_000_000,
            max_catalog_records: 16_000_000,
            max_relationships: 64_000_000,
            max_governance_records: 16_000_000,
            max_channels: 1_000_000,
            max_rank: 32,
            max_nesting_depth: 64,
            max_metadata_bytes: 256 * 1024 * 1024,
            max_logical_payload_bytes: 1_u64 << 60,
        }
    }
}
