use std::error::Error;
use std::fmt;

use crate::TextProjection;
use crate::TextProjector;

enum Utf8Status<'a> {
    Complete(&'a str),
    Prefix(&'a str),
    Invalid {
        valid_up_to: usize,
        error_len: usize,
    },
}

fn classify_utf8(bytes: &[u8]) -> Utf8Status<'_> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Utf8Status::Complete(text),
        Err(error) => match error.error_len() {
            Some(error_len) => Utf8Status::Invalid {
                valid_up_to: error.valid_up_to(),
                error_len,
            },
            None => match std::str::from_utf8(&bytes[..error.valid_up_to()]) {
                Ok(prefix) => Utf8Status::Prefix(prefix),
                Err(prefix_error) => Utf8Status::Invalid {
                    valid_up_to: prefix_error.valid_up_to(),
                    error_len: prefix_error.error_len().unwrap_or_default(),
                },
            },
        },
    }
}

/// Error returned by [`Utf8ProjectionAdapter`] when streamed bytes are not valid UTF-8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Utf8ProjectionError {
    /// The provided bytes contain an invalid UTF-8 sequence.
    InvalidUtf8 {
        /// Byte offset in the parser's buffered bytes where decoding failed.
        valid_up_to: usize,
        /// Length in bytes of the invalid sequence.
        error_len: usize,
    },
    /// EOF was reached with a buffered partial UTF-8 code point.
    IncompleteUtf8AtEof,
}

impl fmt::Display for Utf8ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtf8 {
                valid_up_to,
                error_len,
            } => write!(
                f,
                "invalid UTF-8 in streamed bytes at offset {valid_up_to} (error length {error_len})"
            ),
            Self::IncompleteUtf8AtEof => {
                write!(f, "incomplete UTF-8 code point at end of stream")
            }
        }
    }
}

impl Error for Utf8ProjectionError {}

/// Adapts a [`TextProjector`] to byte-oriented transports while preserving
/// transactional UTF-8 boundaries.
///
/// This is useful when upstream data arrives as `&[u8]` and a code point may be split across
/// chunk boundaries (for example `0xC3` followed by `0xA9` for `é`).
#[derive(Debug)]
pub struct Utf8ProjectionAdapter<P> {
    inner: P,
    pending_utf8: Vec<u8>,
}

impl<P> Utf8ProjectionAdapter<P>
where
    P: TextProjector,
{
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            pending_utf8: Vec::new(),
        }
    }

    /// Feed a raw byte chunk.
    ///
    /// If the chunk contains invalid UTF-8, this returns an error and rolls back the entire
    /// pushed chunk so callers can decide how to recover without the inner parser seeing a partial
    /// prefix from that chunk.
    pub fn push_bytes(
        &mut self,
        chunk: &[u8],
    ) -> Result<TextProjection<P::Event>, Utf8ProjectionError> {
        let old_len = self.pending_utf8.len();
        self.pending_utf8.extend_from_slice(chunk);

        match classify_utf8(&self.pending_utf8) {
            Utf8Status::Complete(text) => {
                let out = self.inner.project(text);
                self.pending_utf8.clear();
                Ok(out)
            }
            Utf8Status::Prefix(prefix) => {
                if prefix.is_empty() {
                    return Ok(TextProjection::default());
                }
                let prefix_len = prefix.len();
                let out = self.inner.project(prefix);
                self.pending_utf8.drain(..prefix_len);
                Ok(out)
            }
            Utf8Status::Invalid {
                valid_up_to,
                error_len,
            } => {
                self.pending_utf8.truncate(old_len);
                Err(Utf8ProjectionError::InvalidUtf8 {
                    valid_up_to,
                    error_len,
                })
            }
        }
    }

    pub fn close(&mut self) -> Result<TextProjection<P::Event>, Utf8ProjectionError> {
        let mut out = if self.pending_utf8.is_empty() {
            TextProjection::default()
        } else {
            match classify_utf8(&self.pending_utf8) {
                Utf8Status::Complete(text) => {
                    let out = self.inner.project(text);
                    self.pending_utf8.clear();
                    out
                }
                Utf8Status::Prefix(_) => return Err(Utf8ProjectionError::IncompleteUtf8AtEof),
                Utf8Status::Invalid {
                    valid_up_to,
                    error_len,
                } => {
                    return Err(Utf8ProjectionError::InvalidUtf8 {
                        valid_up_to,
                        error_len,
                    });
                }
            }
        };
        out.merge(self.inner.close());
        Ok(out)
    }

    /// Return the wrapped parser if no undecoded UTF-8 bytes are buffered.
    ///
    /// Use [`Self::close`] first if you want to flush buffered text into the wrapped projector.
    pub fn into_inner(self) -> Result<P, Utf8ProjectionError> {
        match classify_utf8(&self.pending_utf8) {
            Utf8Status::Complete(_) => Ok(self.inner),
            Utf8Status::Prefix(_) => Err(Utf8ProjectionError::IncompleteUtf8AtEof),
            Utf8Status::Invalid {
                valid_up_to,
                error_len,
            } => Err(Utf8ProjectionError::InvalidUtf8 {
                valid_up_to,
                error_len,
            }),
        }
    }

    /// Return the wrapped parser without validating or flushing buffered undecoded bytes.
    ///
    /// This may drop a partial UTF-8 code point that was buffered across chunk boundaries.
    pub fn into_inner_lossy(self) -> P {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::Utf8ProjectionAdapter;
    use super::Utf8ProjectionError;
    use crate::CitationProjector;
    use crate::TextProjection;
    use crate::TextProjector;

    use pretty_assertions::assert_eq;

    fn collect_bytes(
        parser: &mut Utf8ProjectionAdapter<CitationProjector>,
        chunks: &[&[u8]],
    ) -> Result<TextProjection<String>, Utf8ProjectionError> {
        let mut all = TextProjection::default();
        for chunk in chunks {
            let next = parser.push_bytes(chunk)?;
            all.merge(next);
        }
        all.merge(parser.close()?);
        Ok(all)
    }

    #[test]
    fn utf8_stream_parser_handles_split_code_points_across_chunks() {
        let chunks: [&[u8]; 3] = [
            b"A\xC3",
            b"\xA9<praxis-memory-citation>\xE4",
            b"\xB8\xAD</praxis-memory-citation>Z",
        ];

        let mut parser = Utf8ProjectionAdapter::new(CitationProjector::new());
        let out = match collect_bytes(&mut parser, &chunks) {
            Ok(out) => out,
            Err(err) => panic!("valid UTF-8 stream should parse: {err}"),
        };

        assert_eq!(out.renderable, "AéZ");
        assert_eq!(out.events, vec!["中".to_string()]);
    }

    #[test]
    fn utf8_stream_parser_rolls_back_on_invalid_utf8_chunk() {
        let mut parser = Utf8ProjectionAdapter::new(CitationProjector::new());

        let first = match parser.push_bytes(&[0xC3]) {
            Ok(out) => out,
            Err(err) => panic!("leading byte may be buffered until next chunk: {err}"),
        };
        assert!(first.is_empty());

        let err = match parser.push_bytes(&[0x28]) {
            Ok(out) => panic!("invalid continuation byte should error, got output: {out:?}"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            Utf8ProjectionError::InvalidUtf8 {
                valid_up_to: 0,
                error_len: 1,
            }
        );

        let second = match parser.push_bytes(&[0xA9, b'x']) {
            Ok(out) => out,
            Err(err) => panic!("state should still allow a valid continuation: {err}"),
        };
        let tail = match parser.close() {
            Ok(out) => out,
            Err(err) => panic!("stream should finish: {err}"),
        };

        assert_eq!(second.renderable, "éx");
        assert!(second.events.is_empty());
        assert!(tail.is_empty());
    }

    #[test]
    fn utf8_stream_parser_rolls_back_entire_chunk_when_invalid_byte_follows_valid_prefix() {
        let mut parser = Utf8ProjectionAdapter::new(CitationProjector::new());

        let err = match parser.push_bytes(b"ok\xFF") {
            Ok(out) => panic!("invalid byte should error, got output: {out:?}"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            Utf8ProjectionError::InvalidUtf8 {
                valid_up_to: 2,
                error_len: 1,
            }
        );

        let next = match parser.push_bytes(b"!") {
            Ok(out) => out,
            Err(err) => panic!("parser should recover after rollback: {err}"),
        };

        assert_eq!(next.renderable, "!");
        assert!(next.events.is_empty());
    }

    #[test]
    fn utf8_stream_parser_errors_on_incomplete_code_point_at_eof() {
        let mut parser = Utf8ProjectionAdapter::new(CitationProjector::new());

        let out = match parser.push_bytes(&[0xE2, 0x82]) {
            Ok(out) => out,
            Err(err) => panic!("partial code point should be buffered: {err}"),
        };
        assert!(out.is_empty());

        let err = match parser.close() {
            Ok(out) => panic!("unfinished code point should error, got output: {out:?}"),
            Err(err) => err,
        };
        assert_eq!(err, Utf8ProjectionError::IncompleteUtf8AtEof);
    }

    #[test]
    fn utf8_stream_parser_into_inner_errors_when_partial_code_point_is_buffered() {
        let mut parser = Utf8ProjectionAdapter::new(CitationProjector::new());

        let out = match parser.push_bytes(&[0xC3]) {
            Ok(out) => out,
            Err(err) => panic!("partial code point should be buffered: {err}"),
        };
        assert!(out.is_empty());

        let err = match parser.into_inner() {
            Ok(_) => panic!("buffered partial code point should be rejected"),
            Err(err) => err,
        };
        assert_eq!(err, Utf8ProjectionError::IncompleteUtf8AtEof);
    }

    #[test]
    fn utf8_stream_parser_into_inner_lossy_drops_buffered_partial_code_point() {
        let mut parser = Utf8ProjectionAdapter::new(CitationProjector::new());

        let out = match parser.push_bytes(&[0xC3]) {
            Ok(out) => out,
            Err(err) => panic!("partial code point should be buffered: {err}"),
        };
        assert!(out.is_empty());

        let mut inner = parser.into_inner_lossy();
        let tail = inner.close();
        assert!(tail.is_empty());
    }
}
