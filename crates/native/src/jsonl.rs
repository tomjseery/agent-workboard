use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::{AdapterFailure, AdapterFailureKind, FileFingerprint, ScanCursor, ScanLimits};

const FINGERPRINT_BYTES: u64 = 256;

pub fn discover_jsonl_files(
    root: &Path,
    max_sources: usize,
) -> Result<Vec<std::path::PathBuf>, AdapterFailure> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|error| failure(&directory, 0, AdapterFailureKind::Io, error))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| failure(&directory, 0, AdapterFailureKind::Io, error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| failure(&entry.path(), 0, AdapterFailureKind::Io, error))?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|value| value == "jsonl")
            {
                if files.len() == max_sources {
                    return Err(AdapterFailure {
                        path: root.to_path_buf(),
                        byte_offset: 0,
                        kind: AdapterFailureKind::SourceLimitExceeded,
                        message: format!("native root exceeds {max_sources} JSONL sources"),
                    });
                }
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}

#[derive(Debug)]
pub struct JsonlRecord {
    pub offset: u64,
    pub value: serde_json::Value,
}

#[derive(Debug)]
pub struct JsonlStream {
    pub records: Vec<JsonlRecord>,
    pub start_offset: u64,
    pub source_size: u64,
    pub modified_at_ns: Option<i128>,
    pub cursor: ScanCursor,
    pub incomplete_tail: bool,
}

pub fn stream_jsonl(
    path: &Path,
    prior: Option<&ScanCursor>,
    limits: ScanLimits,
) -> Result<JsonlStream, AdapterFailure> {
    let mut file =
        File::open(path).map_err(|error| failure(path, 0, AdapterFailureKind::Io, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| failure(path, 0, AdapterFailureKind::Io, error))?;
    let source_size = metadata.len();
    let modified_at_ns = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| i128::try_from(value.as_nanos()).unwrap_or(i128::MAX));
    let start_offset = validated_start_offset(&mut file, source_size, prior)
        .map_err(|error| failure(path, 0, AdapterFailureKind::Io, error))?;
    file.seek(SeekFrom::Start(start_offset))
        .map_err(|error| failure(path, start_offset, AdapterFailureKind::Io, error))?;

    let mut reader = BufReader::new(file);
    let mut records = Vec::new();
    let mut offset = start_offset;
    let mut checkpoint_offset = start_offset;
    let mut incomplete_tail = false;

    loop {
        let mut bytes = Vec::new();
        let read = Read::by_ref(&mut reader)
            .take(u64::try_from(limits.max_record_bytes).unwrap_or(u64::MAX) + 2)
            .read_until(b'\n', &mut bytes)
            .map_err(|error| failure(path, offset, AdapterFailureKind::Io, error))?;
        if read == 0 {
            break;
        }
        if bytes.len() > limits.max_record_bytes + 1 {
            return Err(AdapterFailure {
                path: path.to_path_buf(),
                byte_offset: offset,
                kind: AdapterFailureKind::RecordTooLarge,
                message: format!("JSONL record exceeds {} bytes", limits.max_record_bytes),
            });
        }
        if bytes.last() != Some(&b'\n') {
            incomplete_tail = true;
            break;
        }
        let record_offset = offset;
        offset += u64::try_from(read).unwrap_or(u64::MAX);
        checkpoint_offset = offset;
        while matches!(bytes.last(), Some(b'\n' | b'\r')) {
            bytes.pop();
        }
        if bytes.is_empty() {
            continue;
        }
        if records.len() == limits.max_records {
            return Err(AdapterFailure {
                path: path.to_path_buf(),
                byte_offset: record_offset,
                kind: AdapterFailureKind::RecordLimitExceeded,
                message: format!(
                    "JSONL source exceeds {} records per scan",
                    limits.max_records
                ),
            });
        }
        let value = serde_json::from_slice(&bytes).map_err(|error| AdapterFailure {
            path: path.to_path_buf(),
            byte_offset: record_offset,
            kind: AdapterFailureKind::InvalidJson,
            message: format!("{:?}", error.classify()),
        })?;
        records.push(JsonlRecord {
            offset: record_offset,
            value,
        });
    }

    let fingerprint = fingerprint(reader.get_mut(), checkpoint_offset)
        .map_err(|error| failure(path, checkpoint_offset, AdapterFailureKind::Io, error))?;
    Ok(JsonlStream {
        records,
        start_offset,
        source_size,
        modified_at_ns,
        cursor: ScanCursor {
            byte_offset: checkpoint_offset,
            source_size,
            fingerprint,
        },
        incomplete_tail,
    })
}

fn validated_start_offset(
    file: &mut File,
    source_size: u64,
    prior: Option<&ScanCursor>,
) -> std::io::Result<u64> {
    let Some(prior) = prior else {
        return Ok(0);
    };
    if source_size < prior.byte_offset || fingerprint(file, prior.byte_offset)? != prior.fingerprint
    {
        return Ok(0);
    }
    Ok(prior.byte_offset)
}

fn fingerprint(file: &mut File, checkpoint_offset: u64) -> std::io::Result<FileFingerprint> {
    let head_len = checkpoint_offset.min(FINGERPRINT_BYTES);
    let tail_len = checkpoint_offset.min(FINGERPRINT_BYTES);
    let mut head = vec![0; usize::try_from(head_len).unwrap_or(0)];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut head)?;
    let mut tail = vec![0; usize::try_from(tail_len).unwrap_or(0)];
    file.seek(SeekFrom::Start(checkpoint_offset - tail_len))?;
    file.read_exact(&mut tail)?;
    Ok(FileFingerprint {
        head_len,
        head_hash: hash(&head),
        tail_len,
        tail_hash: hash(&tail),
    })
}

fn hash(bytes: &[u8]) -> String {
    let value = bytes.iter().fold(0xcbf29ce484222325_u64, |value, byte| {
        (value ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("{value:016x}")
}

fn failure(
    path: &Path,
    byte_offset: u64,
    kind: AdapterFailureKind,
    error: impl std::fmt::Display,
) -> AdapterFailure {
    AdapterFailure {
        path: path.to_path_buf(),
        byte_offset,
        kind,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Write;

    use tempfile::TempDir;

    use super::stream_jsonl;
    use crate::{AdapterFailureKind, ScanLimits};

    #[test]
    fn retains_an_incomplete_tail_until_it_is_completed() {
        let directory = TempDir::new().expect("the fixture directory should exist");
        let path = directory.path().join("fixture.jsonl");
        fs::write(&path, b"{\"type\":\"first\"}\n{\"type\":\"second\"")
            .expect("the fixture should be written");
        let first = stream_jsonl(&path, None, ScanLimits::default()).expect("the scan should work");

        assert_eq!(first.records.len(), 1);
        assert!(first.incomplete_tail);
        assert_eq!(first.cursor.byte_offset, 17);

        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("the fixture should reopen");
        file.write_all(b"}\n").expect("the tail should complete");
        let second = stream_jsonl(&path, Some(&first.cursor), ScanLimits::default())
            .expect("the incremental scan should work");

        assert_eq!(second.records.len(), 1);
        assert_eq!(second.records[0].value["type"], "second");
        assert!(!second.incomplete_tail);
    }

    #[test]
    fn rejects_a_record_over_the_configured_bound() {
        let directory = TempDir::new().expect("the fixture directory should exist");
        let path = directory.path().join("fixture.jsonl");
        fs::write(&path, b"{\"value\":\"too long\"}\n").expect("the fixture should be written");
        let limits = ScanLimits {
            max_record_bytes: 10,
            ..ScanLimits::default()
        };

        let error = stream_jsonl(&path, None, limits).expect_err("the record should be rejected");

        assert_eq!(error.kind, AdapterFailureKind::RecordTooLarge);
    }
}
