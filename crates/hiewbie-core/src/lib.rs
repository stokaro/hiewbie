//! Portable documents, executable metadata, instruction decoding, and commands.
//!
//! This crate uses bounded random access and has no filesystem or terminal
//! dependency. Native and browser adapters implement [`ByteSource`] and own
//! persistence. Editing is overwrite-only; interpretation always reads the same
//! logical bytes and never executes inspected code.

mod analysis;
mod block;
mod bookmark;
pub mod calculator;
pub mod capabilities;
mod command;
mod document;
mod error;
pub mod formats;
pub mod instructions;
mod offset;
mod references;
mod search;
pub mod symbols;

pub use analysis::{BRANCH_LABELS, CodeRow, DecodeContext, MAX_CODE_ROWS, MAX_NAVIGATION_HISTORY};
pub use block::{BlockBoundary, BlockSelection};
pub use bookmark::{BOOKMARK_SLOTS, BookmarkCommand, BookmarkMark, BookmarkViewport};
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
