mod jsonl;
mod model;

pub use jsonl::{JsonlRecord, JsonlStream, discover_jsonl_files, stream_jsonl};
pub use model::{
    AdapterFailure, AdapterFailureKind, AdapterScan, ConversationKind, CwdObservation,
    FileFingerprint, NativeAdapter, NativeConversation, ScanCursor, ScanLimits, SourceScan,
    SourceState, TranscriptBuilder, TranscriptChunk,
};
