//! Portable documents, executable metadata, instruction decoding, and commands.
//!
//! This crate uses bounded random access and has no filesystem or terminal
//! dependency. Native and browser adapters implement [`ByteSource`] and own
//! persistence. Editing is overwrite-only; interpretation always reads the same
//! logical bytes and never executes inspected code.

mod analysis;
mod block;
pub mod calculator;
mod command;
mod document;
mod error;
pub mod formats;
pub mod instructions;
mod offset;
mod search;
pub mod symbols;

pub use analysis::{CodeRow, DecodeContext, MAX_CODE_ROWS, MAX_NAVIGATION_HISTORY};
pub use block::{BlockBoundary, BlockSelection};
pub use command::{COMMANDS, Command, CommandInfo, Editor, Effect, FileNavigation, ViewMode};
pub use document::{
    ByteSource, Document, MAX_EDIT_BYTES, MAX_HISTORY_BYTES, MAX_HISTORY_ENTRIES,
    MAX_PENDING_BYTES, MAX_READ_BYTES, MemorySource, Patch,
};
pub use error::Error;
pub use offset::{FileOffset, parse_goto, parse_virtual_address};
pub use search::{
    MAX_SEARCH_PATTERN_BYTES, Search, SearchCase, SearchDirection, SearchProgress, SearchRepeat,
    SearchRequest,
};

#[cfg(test)]
mod analysis_tests;
#[cfg(test)]
#[path = "../../../tests/fixtures/symbol_images.rs"]
mod symbol_fixtures;
#[cfg(test)]
mod symbol_navigation_tests;
#[cfg(test)]
mod tests;
