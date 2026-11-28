use std::{fmt, io};

/// A checked core operation failed without changing the logical document.
#[derive(Debug)]
pub enum Error {
    /// The backing byte source failed or changed length unexpectedly.
    Io(io::Error),
    /// A byte range does not fit in the document.
    OutOfBounds {
        offset: u64,
        length: u64,
        document_len: u64,
    },
    /// An operation would exceed an explicit resource limit.
    LimitExceeded {
        resource: &'static str,
        limit: usize,
    },
    /// A goto expression could not be interpreted safely.
    InvalidGoto(&'static str),
    /// Virtual addresses require an address-mapping backend.
    UnsupportedAddress,
    /// An interpretation or navigation operation is unavailable or invalid.
    Analysis(String),
    /// A block command is unavailable in the current state.
    Block(&'static str),
    /// Read-only sessions cannot edit or request persistence.
    ReadOnly,
    /// A mutation or save requires an active edit transaction.
    NotEditing,
    /// A search pattern is empty or invalid.
    InvalidPattern(&'static str),
    /// A search belongs to an older document generation.
    StaleSearch { expected: u64, actual: u64 },
    /// The generation counter cannot represent another changed document.
    GenerationExhausted,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "byte source: {error}"),
            Self::OutOfBounds {
                offset,
                length,
                document_len,
            } => write!(
                formatter,
                "range at 0x{offset:X} with length {length} exceeds document length {document_len}"
            ),
            Self::LimitExceeded { resource, limit } => {
                write!(
                    formatter,
                    "{resource} exceeds the configured limit of {limit}"
                )
            }
            Self::InvalidGoto(reason) => write!(formatter, "invalid goto expression: {reason}"),
            Self::UnsupportedAddress => write!(
                formatter,
                "dotted virtual addresses require an address-mapping backend"
            ),
            Self::Analysis(reason) => formatter.write_str(reason),
            Self::Block(reason) => formatter.write_str(reason),
            Self::ReadOnly => formatter.write_str("the document is open read-only"),
            Self::NotEditing => formatter.write_str("begin an edit transaction first"),
            Self::InvalidPattern(reason) => write!(formatter, "invalid search pattern: {reason}"),
            Self::StaleSearch { expected, actual } => write!(
                formatter,
                "search generation {expected} is stale; document generation is {actual}"
            ),
            Self::GenerationExhausted => formatter.write_str("document generation is exhausted"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
