use std::path::PathBuf;

use hiewbie::file::ExportOffset;
use hiewbie_core::formats::FileRange;
use hiewbie_core::{ByteSource, FileOffset, parse_goto};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Dialog, Ui, UiAction, escaped_text, is_control_char};

#[derive(Debug, Eq, PartialEq)]
pub(super) struct BlockExportRequest {
    pub(super) range: FileRange,
    pub(super) path: PathBuf,
    pub(super) offset: ExportOffset,
    pub(super) replace_existing: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BlockField {
    File,
    Offset,
    Format,
}

pub(super) struct BlockWrite {
    pub(super) range: FileRange,
    pub(super) file: String,
    pub(super) offset: String,
    pub(super) focus: BlockField,
    pub(super) message: Option<String>,
    confirming: bool,
}

impl BlockWrite {
    pub(super) fn new(range: FileRange) -> Self {
        Self {
            range,
            file: String::new(),
            offset: String::new(),
            focus: BlockField::File,
            message: None,
            confirming: false,
        }
    }

    fn parse_offset(&self) -> Result<ExportOffset, String> {
        match self.offset.trim() {
            "" => Ok(ExportOffset::Absolute(FileOffset::new(0))),
            "*" => Ok(ExportOffset::Append),
            value if value.starts_with(['+', '-', '.']) => {
                Err("Use an absolute file offset or * to append".into())
            }
            value => parse_goto(value, FileOffset::new(0), u64::MAX)
                .map(ExportOffset::Absolute)
                .map_err(|error| error.to_string()),
        }
    }

    fn request(&self) -> Result<BlockExportRequest, String> {
        if self.file.is_empty() {
            return Err("Enter an output file path".into());
        }
        Ok(BlockExportRequest {
            range: self.range,
            path: PathBuf::from(&self.file),
            offset: self.parse_offset()?,
            replace_existing: self.confirming,
        })
    }

    fn input(&mut self) -> Option<(&mut String, usize)> {
        match self.focus {
            BlockField::File => Some((&mut self.file, 4096)),
            BlockField::Offset => Some((&mut self.offset, 64)),
            BlockField::Format => None,
        }
    }

    fn change_field(&mut self, backward: bool) {
        self.focus = match (self.focus, backward) {
            (BlockField::File, false) | (BlockField::Format, true) => BlockField::Offset,
            (BlockField::Offset, false) | (BlockField::File, true) => BlockField::Format,
            (BlockField::Format, false) | (BlockField::Offset, true) => BlockField::File,
        };
    }

    fn confirm_field(&mut self) -> Result<Option<BlockExportRequest>, String> {
        match self.focus {
            BlockField::File => {
                if self.file.is_empty() {
                    return Err("Enter an output file path".into());
                }
                self.focus = BlockField::Offset;
            }
            BlockField::Offset => {
                self.parse_offset()?;
                self.focus = BlockField::Format;
            }
            BlockField::Format => return self.request().map(Some),
        }
        self.message = None;
        Ok(None)
    }
}

impl<S: ByteSource> Ui<S> {
    pub(super) fn block_write_key(&mut self, mut dialog: BlockWrite, key: KeyEvent) -> UiAction {
        if key.code == KeyCode::Esc || is_control_char(key, 'c') {
            self.notice("Block export dialog canceled");
            return UiAction::Continue;
        }
        if key.code == KeyCode::Enter && key.modifiers.is_empty() {
            match dialog.confirm_field() {
                Ok(Some(request)) => {
                    self.dialog = Some(Dialog::BlockWrite(dialog));
                    return UiAction::ExportBlock(request);
                }
                Ok(None) => {}
                Err(error) => dialog.message = Some(escaped_text(&error)),
            }
        } else {
            dialog.confirming = false;
            dialog.message = None;
            match (key.code, key.modifiers) {
                (KeyCode::Tab | KeyCode::Down, KeyModifiers::NONE) => dialog.change_field(false),
                (KeyCode::Up, KeyModifiers::NONE) | (KeyCode::BackTab, KeyModifiers::SHIFT) => {
                    dialog.change_field(true);
                }
                (KeyCode::Backspace, KeyModifiers::NONE) => {
                    if let Some((input, _)) = dialog.input() {
                        input.pop();
                    }
                }
                _ if is_control_char(key, 'u') => {
                    if let Some((input, _)) = dialog.input() {
                        input.clear();
                    }
                }
                (KeyCode::Char(character), KeyModifiers::NONE | KeyModifiers::SHIFT)
                    if !character.is_control() =>
                {
                    if let Some((input, limit)) = dialog.input() {
                        if input.len() + character.len_utf8() <= limit {
                            input.push(character);
                        } else {
                            dialog.message = Some("Input limit reached".into());
                        }
                    }
                }
                _ => {}
            }
        }
        self.dialog = Some(Dialog::BlockWrite(dialog));
        UiAction::Continue
    }

    pub(super) fn confirm_block_output(&mut self) {
        if let Some(Dialog::BlockWrite(dialog)) = &mut self.dialog {
            dialog.confirming = true;
            dialog.message = Some("Existing file. Enter confirms the update. Esc cancels.".into());
        }
    }

    pub(super) fn report_export_failure(&mut self, error: impl AsRef<str>) {
        self.export_in_progress = false;
        if let Some(Dialog::BlockWrite(dialog)) = &mut self.dialog {
            dialog.confirming = false;
            dialog.message = Some(escaped_text(error.as_ref()));
        }
    }
}
