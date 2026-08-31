#![forbid(unsafe_code)]

pub mod generation;
mod identity;
mod projection;
mod wire;

pub use identity::*;
pub use projection::*;
pub use wire::*;

pub const CURRENT_PROTOCOL_VERSION: u32 = 7;
pub const PREVIOUS_PROTOCOL_VERSION: u32 = 6;
pub const SUPPORTED_READ_VERSIONS: [u32; 2] = [CURRENT_PROTOCOL_VERSION, PREVIOUS_PROTOCOL_VERSION];
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_COLLECTION_ITEMS: usize = 10_000;
pub const MAX_QUERY_PAGE_ITEMS: usize = 500;
pub const MAX_DIAGNOSTICS: usize = 128;
pub const MAX_AVAILABLE_ACTIONS: usize = 128;
pub const MAX_PARTIAL_OUTCOMES: usize = 1_024;
pub const MAX_REPLAY_EVENTS: usize = 1_024;
pub const EVENT_JOURNAL_RETENTION: u64 = 4_096;

pub fn negotiate_read_version(client_versions: &[u32]) -> Option<u32> {
    SUPPORTED_READ_VERSIONS
        .into_iter()
        .find(|version| client_versions.contains(version))
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn compatibility_table_selects_current_then_previous_and_refuses_unknown() {
        assert_eq!(
            negotiate_read_version(&[PREVIOUS_PROTOCOL_VERSION, CURRENT_PROTOCOL_VERSION]),
            Some(CURRENT_PROTOCOL_VERSION)
        );
        assert_eq!(
            negotiate_read_version(&[PREVIOUS_PROTOCOL_VERSION]),
            Some(PREVIOUS_PROTOCOL_VERSION)
        );
        assert_eq!(negotiate_read_version(&[99]), None);
    }
}
