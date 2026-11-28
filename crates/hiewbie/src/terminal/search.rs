use std::fmt::Write;

use hiewbie_core::formats::FileRange;
use hiewbie_core::{
    ByteSource, Command, MAX_SEARCH_PATTERN_BYTES, SearchCase, SearchDirection, SearchProgress,
    SearchRepeat, SearchRequest, ViewMode,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Dialog, Ui, escaped_text, is_control_char};

pub(super) const SEARCH_STEP_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SearchField {
    Text,
    Hex,
}

pub(super) struct SearchDialog {
    pub(super) field: SearchField,
    pub(super) direction: SearchDirection,
    pub(super) scope: Option<FileRange>,
    pub(super) input: String,
    pub(super) error: Option<String>,
    pub(super) help: Option<super::Help>,
}

impl SearchDialog {
    fn request(&self) -> Result<SearchRequest, String> {
        let mut request = match self.field {
            SearchField::Text => {
                SearchRequest::utf8(self.input.clone(), SearchCase::AsciiInsensitive)
            }
            SearchField::Hex => SearchRequest::bytes(parse_hex_pattern(&self.input)?),
        }
        .map_err(|error| error.to_string())?;
        request.direction = self.direction;
        if let Some(scope) = self.scope {
            request.scope = Some(scope.offset..scope.end().map_err(|error| error.to_string())?);
        }
        Ok(request)
    }

    pub(super) fn alternate_input(&self) -> Result<String, String> {
        if self.input.is_empty() {
            return Ok(String::new());
        }
        match self.field {
            SearchField::Text => {
                let mut hex = String::with_capacity(self.input.len() * 3);
                for byte in self.input.bytes() {
                    if !hex.is_empty() {
                        hex.push(' ');
                    }
                    let _ = write!(hex, "{byte:02X}");
                }
                Ok(hex)
            }
            SearchField::Hex => {
                let text = String::from_utf8(parse_hex_pattern(&self.input)?)
                    .map_err(|_| "Hex query is not valid UTF-8; Ctrl+U clears it")?;
                if text.chars().any(char::is_control) {
                    return Err("Control bytes require the Hex field; Ctrl+U clears it".into());
                }
                Ok(text)
            }
        }
    }
}

impl<S: ByteSource> Ui<S> {
    pub(super) fn open_search_prompt(&mut self) {
        self.dialog = Some(Dialog::Search(SearchDialog {
            field: if self.editor.mode() == ViewMode::Text {
                SearchField::Text
            } else {
                SearchField::Hex
            },
            direction: SearchDirection::Forward,
            scope: None,
            input: String::new(),
            error: None,
            help: None,
        }));
    }

    pub(super) fn search_key(&mut self, mut dialog: SearchDialog, key: KeyEvent) {
        if let Some(help) = &mut dialog.help {
            if help.closes(key) {
                dialog.help = None;
            }
        } else if is_control_char(key, 'c') || key.code == KeyCode::Esc && key.modifiers.is_empty()
        {
            self.hint("Search dialog canceled");
            return;
        } else if is_control_char(key, 'u') {
            dialog.input.clear();
            dialog.error = None;
        } else {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, KeyModifiers::NONE) => match dialog.request() {
                    Ok(request) => {
                        self.editor.start_search(request);
                        self.scan_position = None;
                        self.hint("Searching; Esc cancels");
                        return;
                    }
                    Err(error) => dialog.error = Some(escaped_text(&error)),
                },
                (KeyCode::F(1), KeyModifiers::NONE) => {
                    dialog.help = Some(super::Help::new(super::HelpTopic::Search));
                }
                (KeyCode::F(2), KeyModifiers::NONE) => {
                    dialog.direction = match dialog.direction {
                        SearchDirection::Forward => SearchDirection::Backward,
                        SearchDirection::Backward => SearchDirection::Forward,
                    };
                }
                (KeyCode::F(4), KeyModifiers::NONE) => {
                    if dialog.scope.take().is_some() {
                        dialog.error = None;
                    } else if let Some(block) =
                        self.editor.block_selection().filter(|b| !b.is_marking)
                    {
                        dialog.scope = Some(block.range);
                        dialog.error = None;
                    } else {
                        dialog.error =
                            Some("Finish marking a block with * before selecting its area".into());
                    }
                }
                (KeyCode::F(5), KeyModifiers::NONE) => {
                    if let Some(offset) = self.editor.last_search_hit() {
                        self.dispatch(Command::Goto(offset));
                        self.notice(format!("Last match at {:016X}", offset.get()));
                        return;
                    }
                    dialog.error = Some("No match recorded for the current document bytes".into());
                }
                (KeyCode::Tab | KeyCode::Up | KeyCode::Down, KeyModifiers::NONE)
                | (KeyCode::BackTab, KeyModifiers::SHIFT) => match dialog.alternate_input() {
                    Ok(input) => {
                        dialog.input = input;
                        dialog.field = match dialog.field {
                            SearchField::Text => SearchField::Hex,
                            SearchField::Hex => SearchField::Text,
                        };
                        dialog.error = None;
                    }
                    Err(error) => dialog.error = Some(error),
                },
                (KeyCode::Backspace, KeyModifiers::NONE) => {
                    dialog.input.pop();
                    dialog.error = None;
                }
                (KeyCode::Char(character), KeyModifiers::NONE | KeyModifiers::SHIFT)
                    if !character.is_control() =>
                {
                    let limit = match dialog.field {
                        SearchField::Text => MAX_SEARCH_PATTERN_BYTES,
                        SearchField::Hex => MAX_SEARCH_PATTERN_BYTES * 3,
                    };
                    if dialog.input.len() + character.len_utf8() <= limit {
                        dialog.input.push(character);
                        dialog.error = None;
                    } else {
                        dialog.error = Some("Input limit reached".into());
                    }
                }
                _ => {}
            }
        }
        self.dialog = Some(Dialog::Search(dialog));
    }

    pub(super) fn repeat_search(&mut self) {
        match self.editor.repeat_search() {
            SearchRepeat::Started => {
                self.scan_position = None;
                self.hint("Searching; Esc cancels");
            }
            SearchRepeat::NoQuery => self.notice("No previous search; F7/Ctrl+F opens search"),
            SearchRepeat::Exhausted => {
                self.hint("No further match; search does not wrap");
                self.dialog = Some(Dialog::SearchNotice("Target not found"));
            }
        }
    }

    pub(super) fn advance_search(&mut self) {
        let references = self.editor.is_searching_references();
        let previous_mode = self.editor.mode();
        match self.editor.step_search(SEARCH_STEP_BYTES) {
            Ok(Some(SearchProgress::Pending { next })) => {
                self.scan_position = Some(next);
                self.hint(format!("Scanning {:016X}; Esc cancels", next.get()));
            }
            Ok(Some(SearchProgress::Found(offset))) if references => {
                if previous_mode == ViewMode::Hex {
                    self.top = offset.get();
                }
                self.hint(format!(
                    "Reference at {:016X}; Ctrl+F6 finds the next",
                    offset.get()
                ));
            }
            Ok(Some(SearchProgress::Found(offset))) => self.hint(format!(
                "Found at {:016X}; Shift+F7/Ctrl+R repeats",
                offset.get()
            )),
            Ok(Some(SearchProgress::NotFound)) => {
                self.hint("No match; search does not wrap");
                self.dialog = Some(if references {
                    self.scan_position = self.editor.reference_scan_position();
                    Dialog::ReferenceMiss
                } else {
                    Dialog::SearchNotice("Target not found")
                });
            }
            Ok(None) => {}
            Err(error) => self.notice(format!("Search stopped: {error}")),
        }
    }
}

pub(super) fn parse_hex_pattern(input: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut high = None;
    for character in input.chars() {
        if character.is_ascii_whitespace() {
            continue;
        }
        let Some(digit) = character.to_digit(16).filter(|_| character.is_ascii()) else {
            return Err("Use pairs of hexadecimal digits, optionally separated by spaces".into());
        };
        if let Some(first) = high.take() {
            if bytes.len() == MAX_SEARCH_PATTERN_BYTES {
                return Err(format!(
                    "Search pattern exceeds {MAX_SEARCH_PATTERN_BYTES} bytes"
                ));
            }
            bytes.push((first << 4) | digit as u8);
        } else {
            high = Some(digit as u8);
        }
    }
    if high.is_some() {
        return Err("A hex search requires two digits for every byte".into());
    }
    if bytes.is_empty() {
        return Err("Search pattern must not be empty".into());
    }
    Ok(bytes)
}
