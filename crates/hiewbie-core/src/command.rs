use std::fmt;

use crate::analysis::{Analysis, Location, analysis_error};
use crate::formats::{Endian, Executable, VirtualAddress};
use crate::instructions::DecodeMode;
use crate::symbols::{SymbolTable, SymbolTarget};
use crate::{ByteSource, Document, Error, FileOffset};
use crate::{CodeRow, DecodeContext, MAX_CODE_ROWS, parse_goto, parse_virtual_address};

/// Views sharing one logical document and file cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewMode {
    Hex,
    Text,
    Code,
}

impl fmt::Display for ViewMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Hex => "Hex",
            Self::Text => "Text",
            Self::Code => "Code",
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
    GotoVirtual(VirtualAddress),
    GotoEntry,
    GotoRegion(usize),
    GotoSymbol(usize),
    GotoSymbolRecord(usize),
    SelectSlice(usize),
    SetDecodeMode(Option<DecodeMode>),
    SetEndian(Option<Endian>),
    SetRawBase(VirtualAddress),
    SetRaw(bool),
    FollowBranch(usize),
    ReturnBranch,
    MoveCode(i32),
    ResyncCode,
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
            Self::GotoVirtual(_) => "navigate.virtual",
            Self::GotoEntry => "navigate.entry",
            Self::GotoRegion(_) => "navigate.region",
            Self::GotoSymbol(_) => "navigate.symbol",
            Self::GotoSymbolRecord(_) => "navigate.symbol-record",
            Self::SelectSlice(_) => "analysis.select-slice",
            Self::SetDecodeMode(_) => "analysis.set-architecture",
            Self::SetEndian(_) => "analysis.set-endian",
            Self::SetRawBase(_) => "analysis.set-raw-base",
            Self::SetRaw(_) => "analysis.set-raw",
            Self::FollowBranch(_) => "navigate.follow-branch",
            Self::ReturnBranch => "navigate.return-branch",
            Self::MoveCode(_) => "navigate.code",
            Self::ResyncCode => "analysis.resynchronize",
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

/// Stable implemented command registry. Textual assembly is a separate capability.
pub const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        id: "navigate.symbol",
        description: "Follow a file-backed symbol definition or import pointer slot",
    },
    CommandInfo {
        id: "navigate.symbol-record",
        description: "Follow the backing metadata record for a symbol",
    },
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
        id: "navigate.virtual",
        description: "Translate an absolute virtual address to file bytes",
    },
    CommandInfo {
        id: "navigate.entry",
        description: "Go to the selected image's file-backed entry point",
    },
    CommandInfo {
        id: "navigate.region",
        description: "Go to a selected file-backed section or segment",
    },
    CommandInfo {
        id: "analysis.select-slice",
        description: "Select a universal Mach-O image independently of host CPU",
    },
    CommandInfo {
        id: "analysis.set-architecture",
        description: "Select an instruction architecture or restore automatic inference",
    },
    CommandInfo {
        id: "analysis.set-endian",
        description: "Override instruction byte order or restore the declared order",
    },
    CommandInfo {
        id: "analysis.set-raw-base",
        description: "Select raw interpretation with an explicit virtual base",
    },
    CommandInfo {
        id: "analysis.set-raw",
        description: "Select raw interpretation or restore executable detection",
    },
    CommandInfo {
        id: "navigate.follow-branch",
        description: "Follow a numbered direct destination from freshly decoded bytes",
    },
    CommandInfo {
        id: "navigate.return-branch",
        description: "Restore the file position and interpretation before a branch",
    },
    CommandInfo {
        id: "navigate.code",
        description: "Advance decoded instructions or retrace visited boundaries",
    },
    CommandInfo {
        id: "analysis.resynchronize",
        description: "Restart decoding at the current file offset",
    },
    CommandInfo {
        id: "view.set-mode",
        description: "Select the Hex, Text, or Code view",
    },
    CommandInfo {
        id: "view.cycle-mode",
        description: "Cycle through Hex, Text, and Code views",
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
    analysis: Analysis,
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
            analysis: Analysis::default(),
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

    /// Inspect the current logical generation; parser failures preserve raw access.
    pub fn refresh_analysis(&mut self) {
        self.analysis.refresh(&mut self.document);
    }

    /// Read bounded symbol tables only when requested, independently of Code.
    pub fn refresh_symbols(&mut self) {
        self.analysis.refresh_symbols(&mut self.document);
    }

    #[must_use]
    pub fn symbols(&self) -> Option<&SymbolTable> {
        self.analysis
            .symbols
            .as_ref()
            .filter(|_| self.analysis.is_current(self.document.generation()))
    }

    #[must_use]
    pub fn symbol_problem(&self) -> Option<&str> {
        self.analysis
            .symbol_problem
            .as_deref()
            .filter(|_| self.analysis.is_current(self.document.generation()))
    }

    #[must_use]
    pub fn executable(&self) -> Option<&Executable> {
        self.analysis
            .executable
            .as_ref()
            .filter(|_| self.analysis.is_current(self.document.generation()))
    }

    #[must_use]
    pub fn analysis_problem(&self) -> Option<&str> {
        self.analysis
            .problem
            .as_deref()
            .filter(|_| self.analysis.is_current(self.document.generation()))
    }

    /// Call `refresh_analysis` first when asking for inferred metadata directly.
    #[must_use]
    pub fn decode_context(&self) -> DecodeContext {
        self.analysis.context_at(self.document.generation())
    }

    pub fn code_rows(&mut self, limit: usize) -> Result<Vec<CodeRow>, Error> {
        let rows = self.analysis.rows(&mut self.document, self.cursor, limit)?;
        self.analysis.numbered_window = Some(limit);
        Ok(rows)
    }

    /// All adapters share the same raw/dotted-VA syntax and navigation rules.
    pub fn goto_expression(&mut self, expression: &str) -> Result<Effect, Error> {
        if expression.trim().starts_with('.') {
            self.dispatch(Command::GotoVirtual(parse_virtual_address(expression)?))
        } else {
            self.dispatch(Command::Goto(parse_goto(
                expression,
                self.cursor,
                self.document.len(),
            )?))
        }
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
                self.analysis.walk.clear();
                let offset = if delta < 0 {
                    self.cursor.get().saturating_sub(delta.unsigned_abs())
                } else {
                    self.cursor.get().saturating_add(delta as u64)
                };
                self.cursor = self.clamp(offset);
            }
            Command::Goto(offset) => {
                self.analysis.symbol_mode = None;
                self.analysis.walk.clear();
                self.cursor = self.clamp(offset.get());
            }
            Command::GotoVirtual(address) => {
                self.refresh_analysis();
                let offset = self.analysis.virtual_offset(address, self.document.len())?;
                self.analysis.symbol_mode = None;
                self.analysis.walk.clear();
                self.cursor = offset;
            }
            Command::GotoEntry => {
                self.refresh_analysis();
                let offset = self.analysis.entry_offset(self.document.len())?;
                self.analysis.symbol_mode = None;
                self.analysis.walk.clear();
                self.cursor = offset;
            }
            Command::GotoRegion(index) => {
                self.refresh_analysis();
                let region = self
                    .executable()
                    .and_then(|image| image.regions.get(index))
                    .ok_or_else(|| Error::Analysis("no such executable region".into()))?;
                if region.file.len == 0 {
                    return Err(Error::Analysis("this region is not file-backed".into()));
                }
                let offset = region.file.offset;
                self.analysis.symbol_mode = None;
                self.analysis.walk.clear();
                self.cursor = offset;
            }
            Command::SelectSlice(index) => {
                self.cursor = self.analysis.select_slice(&mut self.document, index)?;
            }
            Command::GotoSymbol(index) | Command::GotoSymbolRecord(index) => {
                self.refresh_symbols();
                let table = self.symbols().ok_or_else(|| {
                    analysis_error(
                        self.symbol_problem()
                            .unwrap_or("symbol tables are unavailable"),
                    )
                })?;
                let symbol = table
                    .entries
                    .get(index)
                    .ok_or_else(|| analysis_error("no such symbol record"))?;
                let target = symbol.target;
                let offset = if matches!(command, Command::GotoSymbolRecord(_)) {
                    symbol.record.offset
                } else {
                    if target == SymbolTarget::Unavailable {
                        return Err(analysis_error(
                            "this symbol has no file-backed target; inspect its metadata record instead",
                        ));
                    }
                    target
                        .file_offset(
                            self.executable().ok_or_else(|| {
                                analysis_error("executable mapping is unavailable")
                            })?,
                        )
                        .map_err(analysis_error)?
                };
                if offset.get() >= self.document.len() {
                    return Err(analysis_error("symbol position is outside the document"));
                }
                self.analysis.symbol_mode = if matches!(command, Command::GotoSymbol(_)) {
                    match target {
                        SymbolTarget::Arm(_) => Some(DecodeMode::Arm),
                        SymbolTarget::Thumb(_) => Some(DecodeMode::Thumb),
                        _ => None,
                    }
                } else {
                    None
                };
                self.analysis.walk.clear();
                self.cursor = offset;
            }
            Command::SetDecodeMode(mode) => {
                self.analysis.interpretation.mode = mode;
                self.analysis.symbol_mode = None;
                self.analysis.walk.clear();
            }
            Command::SetEndian(endian) => {
                self.analysis.interpretation.endian = endian;
                self.analysis.walk.clear();
            }
            Command::SetRawBase(base) => {
                self.analysis.interpretation.base = Some(base);
                self.analysis.interpretation.raw = true;
                self.analysis.invalidate();
            }
            Command::SetRaw(raw) => {
                self.analysis.interpretation.raw = raw;
                if !raw {
                    self.analysis.interpretation.base = None;
                }
                self.analysis.invalidate();
            }
            Command::FollowBranch(number) => self.follow_branch(number)?,
            Command::ReturnBranch => self.return_branch()?,
            Command::MoveCode(count) => self.move_code(count)?,
            Command::ResyncCode => {
                self.analysis.walk.clear();
                self.mode = ViewMode::Code;
            }
            Command::SetMode(mode) => self.mode = mode,
            Command::CycleMode => {
                self.mode = match self.mode {
                    ViewMode::Hex => ViewMode::Text,
                    ViewMode::Text => ViewMode::Code,
                    ViewMode::Code => ViewMode::Hex,
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
        self.analysis.invalidate();
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

    fn follow_branch(&mut self, number: usize) -> Result<(), Error> {
        if !(1..=9).contains(&number) {
            return Err(Error::Analysis(
                "branch number must be between 1 and 9".into(),
            ));
        }
        let limit = self.analysis.numbered_window.unwrap_or(MAX_CODE_ROWS);
        let rows = self.code_rows(limit)?;
        let row = rows
            .iter()
            .find(|row| row.branch_number == Some(number))
            .ok_or_else(|| Error::Analysis("no such direct branch in this code window".into()))?;
        let target = row
            .instruction
            .direct_branch
            .as_ref()
            .ok_or_else(|| Error::Analysis("the selected row has no direct branch".into()))?;
        let offset = self
            .analysis
            .virtual_offset(target.address, self.document.len())?;
        let target_mode = target.mode;
        self.analysis.remember(Location {
            offset: self.cursor,
            interpretation: self.analysis.interpretation,
            symbol_mode: self.analysis.symbol_mode,
            generation: self.document.generation(),
        });
        if self.decode_context().mode != Some(target_mode) {
            self.analysis.interpretation.mode = Some(target_mode);
            self.analysis.symbol_mode = None;
        }
        self.analysis.walk.clear();
        self.cursor = offset;
        Ok(())
    }

    fn return_branch(&mut self) -> Result<(), Error> {
        let location = self
            .analysis
            .history
            .back()
            .copied()
            .ok_or_else(|| Error::Analysis("branch-return history is empty".into()))?;
        if location.offset.get() >= self.document.len() {
            return Err(Error::Analysis(
                "saved branch position is outside the document".into(),
            ));
        }
        self.analysis.interpretation = location.interpretation;
        self.analysis.invalidate();
        self.refresh_analysis();
        if location.generation == self.document.generation()
            && self.analysis.is_current(self.document.generation())
        {
            self.analysis.symbol_mode = location.symbol_mode;
        }
        self.analysis.history.pop_back();
        self.cursor = location.offset;
        Ok(())
    }

    fn move_code(&mut self, count: i32) -> Result<(), Error> {
        if count.unsigned_abs() as usize > MAX_CODE_ROWS {
            return Err(Error::LimitExceeded {
                resource: "code navigation steps",
                limit: MAX_CODE_ROWS,
            });
        }
        self.refresh_analysis();
        for _ in 0..count.unsigned_abs() {
            if count > 0 {
                let rows = self.analysis.rows(&mut self.document, self.cursor, 1)?;
                let Some(row) = rows.first() else {
                    break;
                };
                let next = self
                    .cursor
                    .get()
                    .checked_add(row.instruction.length as u64)
                    .ok_or_else(|| analysis_error("code navigation overflow"))?;
                if next >= self.document.len() {
                    break;
                }
                let next = FileOffset::new(next);
                self.analysis.remember_step(self.cursor, next);
                self.cursor = next;
            } else if self
                .analysis
                .walk
                .back()
                .is_some_and(|(_, to)| *to == self.cursor)
            {
                if let Some((previous, _)) = self.analysis.walk.pop_back() {
                    self.cursor = previous;
                }
            } else {
                // Reverse x86 decoding is ambiguous without an observed boundary.
                // Preserve explicit byte/alignment movement instead of inventing
                // an instruction start; forward traversal records exact starts.
                self.analysis.walk.clear();
                let step = self
                    .decode_context()
                    .mode
                    .map_or(1, |mode| mode.alignment()) as u64;
                self.cursor = FileOffset::new(self.cursor.get().saturating_sub(step));
            }
        }
        Ok(())
    }

    fn clamp(&self, offset: u64) -> FileOffset {
        FileOffset::new(offset.min(self.document.len().saturating_sub(1)))
    }
}
