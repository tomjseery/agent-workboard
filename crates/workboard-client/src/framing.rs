use std::io::{Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;
use workboard_client_protocol::MAX_FRAME_BYTES;

use crate::ClientError;

pub fn write_frame<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), ClientError> {
    let body = serde_json::to_vec(value)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(ClientError::FrameTooLarge {
            actual: body.len(),
            limit: MAX_FRAME_BYTES,
        });
    }
    let length = u32::try_from(body.len()).map_err(|_| ClientError::FrameTooLarge {
        actual: body.len(),
        limit: MAX_FRAME_BYTES,
    })?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> Result<T, ClientError> {
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 {
        return Err(ClientError::EmptyFrame);
    }
    if length > MAX_FRAME_BYTES {
        return Err(ClientError::FrameTooLarge {
            actual: length,
            limit: MAX_FRAME_BYTES,
        });
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Read, Write};

    use serde_json::json;
    use workboard_client_protocol::MAX_FRAME_BYTES;

    use super::{read_frame, write_frame};
    use crate::ClientError;

    struct Chunked<T> {
        inner: T,
        chunk: usize,
    }

    impl<T: Read> Read for Chunked<T> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let length = buffer.len().min(self.chunk);
            self.inner.read(&mut buffer[..length])
        }
    }

    impl<T: Write> Write for Chunked<T> {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.inner.write(&buffer[..buffer.len().min(self.chunk)])
        }

        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }

    struct TimeoutReader;

    impl Read for TimeoutReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::TimedOut, "timed out"))
        }
    }

    #[test]
    fn handles_partial_reads_writes_and_multiple_frames() {
        let mut writer = Chunked {
            inner: Vec::new(),
            chunk: 2,
        };
        write_frame(&mut writer, &json!({ "one": 1 })).expect("first frame");
        write_frame(&mut writer, &json!({ "two": 2 })).expect("second frame");
        let mut reader = Chunked {
            inner: Cursor::new(writer.inner),
            chunk: 1,
        };
        assert_eq!(
            read_frame::<serde_json::Value>(&mut reader).expect("first frame"),
            json!({ "one": 1 })
        );
        assert_eq!(
            read_frame::<serde_json::Value>(&mut reader).expect("second frame"),
            json!({ "two": 2 })
        );
    }

    #[test]
    fn accepts_the_exact_frame_limit_and_rejects_over_limit() {
        let exact = vec![b'a'; MAX_FRAME_BYTES - 2];
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &String::from_utf8(exact).expect("ASCII")).expect("exact limit");
        let mut over = Vec::new();
        let error = write_frame(&mut over, &"a".repeat(MAX_FRAME_BYTES)).expect_err("over limit");
        assert!(matches!(error, ClientError::FrameTooLarge { .. }));
    }

    #[test]
    fn rejects_empty_oversized_disconnected_and_malformed_frames() {
        assert!(matches!(
            read_frame::<serde_json::Value>(&mut Cursor::new(0_u32.to_be_bytes())),
            Err(ClientError::EmptyFrame)
        ));
        assert!(matches!(
            read_frame::<serde_json::Value>(&mut Cursor::new(
                ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes()
            )),
            Err(ClientError::FrameTooLarge { .. })
        ));
        assert!(matches!(
            read_frame::<serde_json::Value>(&mut Cursor::new([0_u8, 0, 0])),
            Err(ClientError::Io(_))
        ));
        assert!(matches!(
            read_frame::<serde_json::Value>(&mut Cursor::new([0_u8, 0, 0, 1, b'{'])),
            Err(ClientError::Json(_))
        ));
        let timeout = read_frame::<serde_json::Value>(&mut TimeoutReader)
            .expect_err("timeout must be preserved");
        assert!(matches!(
            timeout,
            ClientError::Io(error) if error.kind() == io::ErrorKind::TimedOut
        ));
    }
}
