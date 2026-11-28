use std::fmt;

use crate::{ByteSource, Document, Error, FileOffset};

/// Views supported by this first raw-byte editor slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewMode {
    Hex,
    Text,
}

impl fmt::Display for ViewMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Hex => "Hex",
            Self::Text => "Text",
        })
    }
}

/// Stable semantic commands shared by interactive and headless adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    BeginEdit,
    DiscardEdit,
    WriteByte(u8),
    RestoreByte,
    RestorePreviousByte,
    Undo,
    Redo,
    Move(i64),
    Goto(FileOffset),
    SetMode(ViewMode),
    CycleMode,
    Save,
}

impl Command {
    /// Persistent command identity, independent of a frontend's physical keys.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        match self {
            Self::BeginEdit => "edit.begin",
            Self::DiscardEdit => "edit.discard",
            Self::WriteByte(_) => "edit.write-byte",
            Self::RestoreByte => "edit.restore-byte",
            Self::RestorePreviousByte => "edit.restore-previous-byte",
            Self::Undo => "edit.undo",
            Self::Redo => "edit.redo",
            Self::Move(_) => "navigate.move",
            Self::Goto(_) => "navigate.goto",
            Self::SetMode(_) => "view.set-mode",
            Self::CycleMode => "view.cycle-mode",
            Self::Save => "file.save",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandInfo {
    pub id: &'static str,
    pub description: &'static str,
}

/// Initial command registry. No unavailable format or Code-mode command is
/// advertised as implemented.
pub const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        id: "edit.begin",
        description: "Begin a pending overwrite transaction",
    },
    CommandInfo {
        id: "edit.discard",
        description: "Discard pending edits and leave edit mode",
    },
    CommandInfo {
        id: "edit.write-byte",
        description: "Overwrite the cursor byte and advance",
    },
    CommandInfo {
        id: "edit.restore-byte",
        description: "Restore the source byte under the cursor without moving",
    },
    CommandInfo {
        id: "edit.restore-previous-byte",
        description: "Restore the previous source byte and move back to it",
    },
    CommandInfo {
        id: "edit.undo",
        description: "Undo one pending byte edit",
    },
    CommandInfo {
        id: "edit.redo",
        description: "Redo one pending byte edit",
    },
    CommandInfo {
        id: "navigate.move",
        description: "Move by a signed byte count, clamped to the document",
    },
    CommandInfo {
        id: "navigate.goto",
        description: "Move to a raw file offset, clamped to the document",
    },
    CommandInfo {
        id: "view.set-mode",
        description: "Select the Hex or Text view",
    },
    CommandInfo {
        id: "view.cycle-mode",
        description: "Cycle between Hex and Text views",
    },
    CommandInfo {
        id: "file.save",
        description: "Request persistence of the active edit transaction",
    },
];

/// Platform work requested by a command. The core never writes files itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    None,
    Save,
}

/// Shared view, cursor, and edit-state semantics around a document.
#[derive(Debug)]
pub struct Editor<S: ByteSource> {
    document: Document<S>,
    cursor: FileOffset,
    mode: ViewMode,
    editing: bool,
    read_only: bool,
}

impl<S: ByteSource> Editor<S> {
    #[must_use]
    pub fn new(document: Document<S>, read_only: bool) -> Self {
        Self {
            document,
            cursor: FileOffset::new(0),
            mode: ViewMode::Hex,
            editing: false,
            read_only,
        }
    }

    #[must_use]
    pub const fn document(&self) -> &Document<S> {
        &self.document
    }

    pub fn document_mut(&mut self) -> &mut Document<S> {
        &mut self.document
    }

    #[must_use]
    pub const fn cursor(&self) -> FileOffset {
        self.cursor
    }

    #[must_use]
    pub const fn mode(&self) -> ViewMode {
        self.mode
    }

    #[must_use]
    pub const fn is_editing(&self) -> bool {
        self.editing
    }

    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn dispatch(&mut self, command: Command) -> Result<Effect, Error> {
        match command {
            Command::BeginEdit => {
                if self.read_only {
                    return Err(Error::ReadOnly);
                }
                if !self.editing {
                    self.document.discard();
                    self.editing = true;
                }
            }
            Command::DiscardEdit => {
                if self.editing {
                    self.document.discard();
                    self.editing = false;
                }
            }
            Command::WriteByte(byte) => {
                self.require_editing()?;
                self.document.overwrite(self.cursor, &[byte])?;
                self.cursor = self.clamp(self.cursor.get().saturating_add(1));
            }
            Command::RestoreByte => {
                self.require_editing()?;
                self.document.restore_byte(self.cursor)?;
            }
            Command::RestorePreviousByte => {
                self.require_editing()?;
                if let Some(previous) = self.cursor.get().checked_sub(1) {
                    let offset = FileOffset::new(previous);
                    self.document.restore_byte(offset)?;
                    self.cursor = offset;
                }
            }
            Command::Undo => {
                self.require_editing()?;
                self.document.undo()?;
            }
            Command::Redo => {
                self.require_editing()?;
                self.document.redo()?;
            }
            Command::Move(delta) => {
                let offset = if delta < 0 {
                    self.cursor.get().saturating_sub(delta.unsigned_abs())
                } else {
                    self.cursor.get().saturating_add(delta as u64)
                };
                self.cursor = self.clamp(offset);
            }
            Command::Goto(offset) => self.cursor = self.clamp(offset.get()),
            Command::SetMode(mode) => self.mode = mode,
            Command::CycleMode => {
                self.mode = match self.mode {
                    ViewMode::Hex => ViewMode::Text,
                    ViewMode::Text => ViewMode::Hex,
                };
            }
            Command::Save => {
                self.require_editing()?;
                return Ok(Effect::Save);
            }
        }
        Ok(Effect::None)
    }

    /// End editing only after the platform adapter has completed a save.
    pub fn complete_save(&mut self, source: S) {
        self.document.rebase(source);
        self.editing = false;
        self.cursor = self.clamp(self.cursor.get());
    }

    fn require_editing(&self) -> Result<(), Error> {
        if self.read_only {
            return Err(Error::ReadOnly);
        }
        if !self.editing {
            return Err(Error::NotEditing);
        }
        Ok(())
    }

    fn clamp(&self, offset: u64) -> FileOffset {
        FileOffset::new(offset.min(self.document.len().saturating_sub(1)))
    }
}
