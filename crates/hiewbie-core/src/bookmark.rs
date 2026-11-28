//! Bounded screen positions for the three document views.

use crate::instructions::DecodeMode;
use crate::{FileOffset, ViewMode};

/// Numbered screen bookmarks available independently in each view.
pub const BOOKMARK_SLOTS: usize = 8;

/// The adapter's logical viewport coordinates at a saved document cursor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BookmarkViewport {
    /// File offset of the first visible row.
    pub top: FileOffset,
    /// Horizontal scroll in Text view, measured in character cells.
    pub column: u16,
}

/// Screen bookmark commands use one-based slot numbers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookmarkCommand {
    Store(BookmarkViewport),
    Recall,
    Select(u8),
    Delete,
    Clear,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Bookmark {
    pub offset: FileOffset,
    pub viewport: BookmarkViewport,
    pub decode_mode: Option<DecodeMode>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ViewBookmarks {
    items: [Option<Bookmark>; BOOKMARK_SLOTS],
    current: usize,
}

impl Default for ViewBookmarks {
    fn default() -> Self {
        Self {
            items: [None; BOOKMARK_SLOTS],
            current: 0,
        }
    }
}

impl ViewBookmarks {
    pub fn store(&mut self, bookmark: Bookmark) {
        let Some(index) = self.items.iter().position(Option::is_none) else {
            return;
        };
        self.items[index] = Some(bookmark);
        self.current = index;
    }

    pub fn recall(&self) -> Option<Bookmark> {
        self.items[self.current]
    }

    pub fn select(&mut self, slot: u8) -> Option<Bookmark> {
        let index = usize::from(slot.checked_sub(1)?);
        let bookmark = self.items.get(index).copied().flatten()?;
        self.current = index;
        Some(bookmark)
    }

    pub fn delete(&mut self) {
        self.items[self.current] = None;
    }

    fn marks(&self) -> [BookmarkMark; BOOKMARK_SLOTS] {
        let mut marks = self.items.map(|item| {
            if item.is_some() {
                BookmarkMark::Stored
            } else {
                BookmarkMark::Empty
            }
        });
        if marks[self.current] == BookmarkMark::Stored {
            marks[self.current] = BookmarkMark::Current;
        }
        marks
    }
}

/// State of one numbered bookmark in the active view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookmarkMark {
    Empty,
    Stored,
    Current,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Bookmarks {
    modes: [ViewBookmarks; 3],
}

impl Bookmarks {
    fn index(mode: ViewMode) -> usize {
        match mode {
            ViewMode::Text => 0,
            ViewMode::Hex => 1,
            ViewMode::Code => 2,
        }
    }

    pub fn view_mut(&mut self, mode: ViewMode) -> &mut ViewBookmarks {
        &mut self.modes[Self::index(mode)]
    }

    pub fn marks(&self, mode: ViewMode) -> [BookmarkMark; BOOKMARK_SLOTS] {
        self.modes[Self::index(mode)].marks()
    }
}
