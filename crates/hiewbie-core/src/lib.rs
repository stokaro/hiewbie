//! Portable raw-byte documents, transactions, commands, and incremental search.
//!
//! This crate uses bounded random access and has no filesystem or terminal
//! dependency. Native and browser adapters implement [`ByteSource`] and own
//! persistence. Initial functionality is overwrite-only; it does not interpret
//! executable formats or claim an instruction-analysis mode.

mod command;
mod document;
mod error;
mod offset;
mod search;

pub use command::{COMMANDS, Command, CommandInfo, Editor, Effect, ViewMode};
pub use document::{
    ByteSource, Document, MAX_EDIT_BYTES, MAX_HISTORY_BYTES, MAX_HISTORY_ENTRIES,
    MAX_PENDING_BYTES, MAX_READ_BYTES, MemorySource, Patch,
};
pub use error::Error;
pub use offset::{FileOffset, parse_goto};
pub use search::{MAX_SEARCH_PATTERN_BYTES, Search, SearchProgress};

#[cfg(test)]
mod tests;
