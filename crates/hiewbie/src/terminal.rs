//! Thin native terminal adapter over the portable document and command model.

mod render;
#[cfg(test)]
mod tests;

use std::io::{self, Stdout};
use std::time::Duration;

use hiewbie::file::{FileSource, SaveError, save_document};
use hiewbie_core::{
    ByteSource, Command, Editor, Effect, FileOffset, MAX_SEARCH_PATTERN_BYTES, Search,
    SearchProgress, ViewMode, parse_goto,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    style::ResetColor,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

const SEARCH_STEP_BYTES: usize = 64 * 1024;

pub(crate) fn printable(byte: u8) -> char {
    if (b' '..=b'~').contains(&byte) {
        char::from(byte)
    } else {
        '.'
    }
}

/// Untrusted filenames and I/O errors must never become terminal instructions.
pub(crate) fn escaped_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control()
            || matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            result.extend(character.escape_default());
        } else {
            result.push(character);
        }
    }
    result
}

/// Each acquisition is marked before writing, because a terminal command can
/// take effect even when the subsequent flush reports an error. Drop also runs
/// during partial initialization and ordinary Rust unwinding.
#[derive(Default)]
struct TerminalGuard {
    raw: bool,
    alternate: bool,
    paste: bool,
    cursor_hidden: bool,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        let mut guard = Self {
            raw: true,
            ..Self::default()
        };
        enable_raw_mode()?;
        guard.alternate = true;
        execute!(io::stdout(), EnterAlternateScreen)?;
        guard.paste = true;
        execute!(io::stdout(), EnableBracketedPaste)?;
        guard.cursor_hidden = true;
        execute!(io::stdout(), Hide)?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Continue restoring independent modes after an individual I/O failure.
        if self.alternate {
            let _ = execute!(io::stdout(), ResetColor);
        }
        if self.cursor_hidden {
            let _ = execute!(io::stdout(), Show);
        }
        if self.paste {
            let _ = execute!(io::stdout(), DisableBracketedPaste);
        }
        if self.alternate {
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
        }
        if self.raw {
            let _ = disable_raw_mode();
        }
    }
}

pub(crate) fn run(editor: Editor<FileSource>) -> io::Result<()> {
    let label = escaped_text(&editor.document().source().path().to_string_lossy());
    let mut ui = Ui::new(editor, label);
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;
    terminal.clear()?;
    let mut redraw = true;

    loop {
        if redraw {
            terminal.draw(|frame| ui.render(frame))?;
        }
        redraw = false;
        let timeout = if ui.search.is_some() {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(100)
        };
        if event::poll(timeout)? {
            let action = match event::read()? {
                Event::Key(key) => ui.key(key),
                Event::Paste(_) => {
                    ui.reject_paste();
                    UiAction::Continue
                }
                Event::Resize(_, _) => UiAction::Continue,
                _ => UiAction::Continue,
            };
            redraw = true;
            match action {
                UiAction::Exit => break,
                UiAction::Save => match save_document(ui.editor.document_mut()) {
                    Ok(source) => {
                        ui.editor.complete_save(source);
                        ui.pending_nibble = None;
                        ui.notice("Saved; edit transaction completed");
                    }
                    Err(error) => ui.save_failed(error),
                },
                UiAction::Continue => {}
            }
        }
        if ui.search.is_some() {
            ui.advance_search();
            redraw = true;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UiAction {
    Continue,
    Save,
    Exit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryColumn {
    Hex,
    Ascii,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PromptKind {
    Goto,
    SearchHex,
    SearchText,
}

struct InputDialog {
    kind: PromptKind,
    input: String,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModeChoice {
    Text,
    Hex,
    Code,
}

enum Dialog {
    Input(InputDialog),
    Help,
    Mode(ModeChoice),
}

struct Ui<S: ByteSource> {
    editor: Editor<S>,
    label: String,
    top: u64,
    bytes_per_row: usize,
    visible_rows: usize,
    entry_column: EntryColumn,
    pending_nibble: Option<u8>,
    message: String,
    dialog: Option<Dialog>,
    search: Option<Search>,
    last_pattern: Option<Vec<u8>>,
    reopen_required: bool,
}

impl<S: ByteSource> Ui<S> {
    fn new(editor: Editor<S>, label: String) -> Self {
        Self {
            editor,
            label: escaped_text(&label),
            top: 0,
            bytes_per_row: 16,
            visible_rows: 21,
            entry_column: EntryColumn::Hex,
            pending_nibble: None,
            message: "F1 help | Enter cycle | F4 select mode | Esc quit | Code planned".into(),
            dialog: None,
            search: None,
            last_pattern: None,
            reopen_required: false,
        }
    }

    fn notice(&mut self, text: impl AsRef<str>) {
        self.message = escaped_text(text.as_ref());
    }

    fn dispatch(&mut self, command: Command) -> UiAction {
        if self.pending_nibble.take().is_some() {
            self.notice("Pending hex digit cancelled; no partial byte was applied");
        }
        match self.editor.dispatch(command) {
            Ok(Effect::None) => UiAction::Continue,
            Ok(Effect::Save) => UiAction::Save,
            Err(error) => {
                self.notice(error.to_string());
                UiAction::Continue
            }
        }
    }

    fn reject_paste(&mut self) {
        self.pending_nibble = None;
        if self.reopen_required {
            return;
        }
        self.notice(
            "Paste rejected: this version accepts typed byte edits only; no pasted commands ran",
        );
    }

    fn save_failed(&mut self, error: SaveError) {
        if let SaveError::ReplacedButRefreshFailed(error) = error {
            // Replacement already happened. Retain the overlay until explicit
            // exit, but do not write through the stale source or claim rollback.
            self.reopen_required = true;
            self.notice(format!(
                "File replaced; reopen required. Esc exits. Verification failed: {error}"
            ));
        } else {
            self.notice(format!("Save failed; pending edits retained: {error}"));
        }
    }

    fn key(&mut self, key: KeyEvent) -> UiAction {
        if key.kind == KeyEventKind::Release {
            return UiAction::Continue;
        }
        if self.reopen_required {
            return if key.code == KeyCode::Esc || is_control_char(key, 'c') {
                UiAction::Exit
            } else {
                UiAction::Continue
            };
        }
        if self.search.is_some() {
            if key.code == KeyCode::Esc || is_control_char(key, 'c') {
                self.search = None;
                self.notice("Search cancelled; cursor unchanged");
            }
            return UiAction::Continue;
        }
        if self.dialog.is_some() {
            self.dialog_key(key);
            return UiAction::Continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return self.control_key(key);
        }
        if key.code == KeyCode::F(7)
            && key.modifiers == KeyModifiers::SHIFT
            && !self.editor.is_editing()
        {
            self.repeat_search();
            return UiAction::Continue;
        }
        if key.modifiers.contains(KeyModifiers::ALT)
            || (!matches!(key.code, KeyCode::Char(_)) && !key.modifiers.is_empty())
        {
            self.pending_nibble = None;
            self.notice("This modified binding is not implemented; F1 lists available commands");
            return UiAction::Continue;
        }

        match key.code {
            KeyCode::Esc => self.cancel_or_exit(),
            KeyCode::F(1) => {
                self.pending_nibble = None;
                self.dialog = Some(Dialog::Help);
                UiAction::Continue
            }
            KeyCode::Left => self.dispatch(Command::Move(-1)),
            KeyCode::Right => self.dispatch(Command::Move(1)),
            KeyCode::Up => self.dispatch(Command::Move(-(self.bytes_per_row as i64))),
            KeyCode::Down => self.dispatch(Command::Move(self.bytes_per_row as i64)),
            KeyCode::PageUp => self.dispatch(Command::Move(-self.page_step())),
            KeyCode::PageDown => self.dispatch(Command::Move(self.page_step())),
            KeyCode::Home => {
                let row = self.editor.cursor().get() / self.bytes_per_row as u64;
                self.dispatch(Command::Goto(FileOffset::new(
                    row * self.bytes_per_row as u64,
                )))
            }
            KeyCode::End => {
                let offset = self.editor.cursor().get();
                let remaining = self.bytes_per_row as u64 - 1 - offset % self.bytes_per_row as u64;
                self.dispatch(Command::Goto(FileOffset::new(
                    offset.saturating_add(remaining),
                )))
            }
            _ if self.editor.is_editing() => self.edit_key(key.code),
            _ => self.view_key(key.code),
        }
    }

    fn page_step(&self) -> i64 {
        (self.visible_rows.max(1) * self.bytes_per_row) as i64
    }

    fn cancel_or_exit(&mut self) -> UiAction {
        if self.editor.is_editing() {
            let action = self.dispatch(Command::DiscardEdit);
            self.notice("Pending edits discarded; viewing original bytes. Esc quits");
            action
        } else if self.editor.document().is_dirty() {
            // No frontend path should create this state. Retain a defensive
            // barrier if a future command leaves an overlay outside edit mode.
            self.notice("Pending edits exist; resolve the edit transaction before exiting");
            UiAction::Continue
        } else {
            UiAction::Exit
        }
    }

    fn begin_edit(&mut self) -> UiAction {
        self.entry_column = match self.editor.mode() {
            ViewMode::Hex => EntryColumn::Hex,
            ViewMode::Text => EntryColumn::Ascii,
        };
        self.notice(
            "Editing: F9 saves; Esc discards; F3 restores byte; Backspace restores previous",
        );
        self.dispatch(Command::BeginEdit)
    }

    fn save(&mut self) -> UiAction {
        if self.pending_nibble.is_some() {
            self.notice(
                "Incomplete hex byte: enter its second digit, or Backspace to cancel the digit",
            );
            return UiAction::Continue;
        }
        self.dispatch(Command::Save)
    }

    fn view_key(&mut self, code: KeyCode) -> UiAction {
        match code {
            KeyCode::Enter => {
                self.notice("Hex/Text mode changed; Code mode is not implemented");
                self.dispatch(Command::CycleMode)
            }
            KeyCode::F(4) => {
                self.dialog = Some(Dialog::Mode(match self.editor.mode() {
                    ViewMode::Text => ModeChoice::Text,
                    ViewMode::Hex => ModeChoice::Hex,
                }));
                UiAction::Continue
            }
            KeyCode::F(3) => self.begin_edit(),
            KeyCode::F(5) => {
                self.open_prompt(PromptKind::Goto);
                UiAction::Continue
            }
            KeyCode::F(7) => {
                self.open_search_prompt();
                UiAction::Continue
            }
            KeyCode::F(9) => {
                self.notice("F9 Open is not implemented; start hiewbie with another file path");
                UiAction::Continue
            }
            KeyCode::Tab => {
                self.notice("Tab next-file is not implemented; only one file is open");
                UiAction::Continue
            }
            KeyCode::F(_) | KeyCode::Insert | KeyCode::Delete => {
                self.notice(
                    "This command is not implemented; F1 lists the available raw-byte commands",
                );
                UiAction::Continue
            }
            _ => UiAction::Continue,
        }
    }

    fn edit_key(&mut self, code: KeyCode) -> UiAction {
        match code {
            KeyCode::F(9) => self.save(),
            KeyCode::Tab => {
                self.pending_nibble = None;
                self.entry_column = match self.entry_column {
                    EntryColumn::Hex => EntryColumn::Ascii,
                    EntryColumn::Ascii => EntryColumn::Hex,
                };
                self.notice(match self.entry_column {
                    EntryColumn::Hex => {
                        "Hex entry: type two digits per byte; F9 saves; Esc discards"
                    }
                    EntryColumn::Ascii => {
                        "ASCII entry: type printable ASCII bytes; F9 saves; Esc discards"
                    }
                });
                UiAction::Continue
            }
            KeyCode::Backspace if self.pending_nibble.is_some() => {
                self.pending_nibble = None;
                self.notice("Pending hex digit cancelled; document unchanged");
                UiAction::Continue
            }
            KeyCode::Backspace => {
                self.notice(if self.editor.cursor().get() == 0 {
                    "No previous byte at start of file"
                } else {
                    "Previous byte restored; F9 saves; Esc discards"
                });
                self.dispatch(Command::RestorePreviousByte)
            }
            KeyCode::Char(character) => self.enter_character(character),
            KeyCode::F(3) => {
                self.pending_nibble = None;
                self.notice("Original byte restored; F9 saves; Esc discards");
                self.dispatch(Command::RestoreByte)
            }
            _ => {
                self.pending_nibble = None;
                self.notice(
                    "Edit binding unavailable; F1 help, F9 save, Esc discard, Tab hex/ASCII",
                );
                UiAction::Continue
            }
        }
    }

    fn enter_character(&mut self, character: char) -> UiAction {
        match self.entry_column {
            EntryColumn::Hex => {
                let Some(digit) = character.to_digit(16).filter(|_| character.is_ascii()) else {
                    self.pending_nibble = None;
                    self.notice("Hex entry requires digits 0-9 or A-F; Tab selects ASCII entry");
                    return UiAction::Continue;
                };
                if let Some(high) = self.pending_nibble.take() {
                    self.notice("Byte overwrite applied; F9/Ctrl+S saves; Esc discards");
                    self.dispatch(Command::WriteByte((high << 4) | digit as u8))
                } else {
                    self.pending_nibble = Some(digit as u8);
                    self.notice(format!(
                        "Pending hex {digit:X}_: enter the second digit; document is unchanged"
                    ));
                    UiAction::Continue
                }
            }
            EntryColumn::Ascii => {
                if !character.is_ascii() || character.is_ascii_control() {
                    self.notice(
                        "ASCII entry accepts printable bytes only; use hex for other bytes",
                    );
                    UiAction::Continue
                } else {
                    self.notice("Byte overwrite applied; F9/Ctrl+S saves; Esc discards");
                    self.dispatch(Command::WriteByte(character as u8))
                }
            }
        }
    }

    fn control_key(&mut self, key: KeyEvent) -> UiAction {
        if key.modifiers.intersects(
            KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::HYPER | KeyModifiers::META,
        ) {
            self.pending_nibble = None;
            self.notice("This modifier combination is not implemented");
            return UiAction::Continue;
        }
        match key.code {
            KeyCode::Home => self.dispatch(Command::Goto(FileOffset::new(0))),
            KeyCode::End => self.dispatch(Command::Goto(FileOffset::new(
                self.editor.document().len().saturating_sub(1),
            ))),
            KeyCode::Char('e' | 'E') => self.begin_edit(),
            KeyCode::Char('s' | 'S') => self.save(),
            KeyCode::Char('z' | 'Z') => self.dispatch(Command::Undo),
            KeyCode::Char('y' | 'Y') => self.dispatch(Command::Redo),
            KeyCode::Char('g' | 'G') if !self.editor.is_editing() => {
                self.open_prompt(PromptKind::Goto);
                UiAction::Continue
            }
            KeyCode::Char('f' | 'F') if !self.editor.is_editing() => {
                self.open_search_prompt();
                UiAction::Continue
            }
            KeyCode::Enter | KeyCode::Char('r' | 'R') if !self.editor.is_editing() => {
                self.repeat_search();
                UiAction::Continue
            }
            KeyCode::Char('c' | 'C') if !self.editor.is_editing() => self.cancel_or_exit(),
            _ => {
                self.pending_nibble = None;
                self.notice(
                    "Binding unavailable in this context; F1 help; Esc cancels or discards editing",
                );
                UiAction::Continue
            }
        }
    }

    fn open_prompt(&mut self, kind: PromptKind) {
        self.pending_nibble = None;
        self.dialog = Some(Dialog::Input(InputDialog {
            kind,
            input: String::new(),
            error: None,
        }));
    }

    fn open_search_prompt(&mut self) {
        self.open_prompt(match self.editor.mode() {
            ViewMode::Hex => PromptKind::SearchHex,
            ViewMode::Text => PromptKind::SearchText,
        });
    }

    fn dialog_key(&mut self, key: KeyEvent) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        if key.code == KeyCode::Esc || is_control_char(key, 'c') {
            self.notice("Dialog cancelled");
            return;
        }
        let mut input = match dialog {
            Dialog::Input(input) => input,
            Dialog::Help => {
                if !matches!(key.code, KeyCode::F(1) | KeyCode::Enter) {
                    self.dialog = Some(Dialog::Help);
                } else {
                    self.notice(
                        "Help closed; Esc cancels or discards editing, and quits from view",
                    );
                }
                return;
            }
            Dialog::Mode(mut selected) => {
                if key.modifiers.is_empty() {
                    selected = match (key.code, selected) {
                        (KeyCode::Up, ModeChoice::Code) => ModeChoice::Hex,
                        (KeyCode::Up, ModeChoice::Hex) => ModeChoice::Text,
                        (KeyCode::Down, ModeChoice::Text) => ModeChoice::Hex,
                        (KeyCode::Down, ModeChoice::Hex) => ModeChoice::Code,
                        _ => selected,
                    };
                    if key.code == KeyCode::Enter {
                        let mode = match selected {
                            ModeChoice::Text => Some(ViewMode::Text),
                            ModeChoice::Hex => Some(ViewMode::Hex),
                            ModeChoice::Code => None,
                        };
                        if let Some(mode) = mode {
                            self.dispatch(Command::SetMode(mode));
                            self.notice(format!("{mode} view selected; cursor unchanged"));
                            return;
                        }
                        self.notice("Code mode is not implemented; select Text or Hex");
                    }
                }
                self.dialog = Some(Dialog::Mode(selected));
                return;
            }
        };
        match key.code {
            KeyCode::Enter => match self.apply_prompt(&input) {
                Ok(()) => return,
                Err(error) => input.error = Some(escaped_text(&error)),
            },
            KeyCode::Backspace => {
                input.input.pop();
                input.error = None;
            }
            KeyCode::Tab => {
                input.kind = match input.kind {
                    PromptKind::SearchHex => PromptKind::SearchText,
                    PromptKind::SearchText => PromptKind::SearchHex,
                    PromptKind::Goto => PromptKind::Goto,
                };
                input.error = None;
            }
            _ if is_control_char(key, 'u') => {
                input.input.clear();
                input.error = None;
            }
            KeyCode::Char(character)
                if !character.is_control()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let limit = if input.kind == PromptKind::Goto {
                    64
                } else {
                    MAX_SEARCH_PATTERN_BYTES * 3
                };
                if input.input.len() + character.len_utf8() <= limit {
                    input.input.push(character);
                    input.error = None;
                } else {
                    input.error = Some("Input limit reached".into());
                }
            }
            _ => {}
        }
        self.dialog = Some(Dialog::Input(input));
    }

    fn apply_prompt(&mut self, prompt: &InputDialog) -> Result<(), String> {
        match prompt.kind {
            PromptKind::Goto => {
                let offset = parse_goto(
                    &prompt.input,
                    self.editor.cursor(),
                    self.editor.document().len(),
                )
                .map_err(|error| error.to_string())?;
                self.editor
                    .dispatch(Command::Goto(offset))
                    .map_err(|error| error.to_string())?;
                self.notice(format!("Offset {:016X}", offset.get()));
                Ok(())
            }
            PromptKind::SearchHex | PromptKind::SearchText => {
                let pattern = if prompt.kind == PromptKind::SearchHex {
                    parse_hex_pattern(&prompt.input)?
                } else {
                    let pattern = prompt.input.as_bytes();
                    if pattern.is_empty() || pattern.len() > MAX_SEARCH_PATTERN_BYTES {
                        return Err(format!(
                            "Text search requires 1..={MAX_SEARCH_PATTERN_BYTES} UTF-8 bytes"
                        ));
                    }
                    pattern.to_vec()
                };
                self.start_search(pattern, self.editor.cursor())
            }
        }
    }

    fn start_search(&mut self, pattern: Vec<u8>, start: FileOffset) -> Result<(), String> {
        let search = Search::new(pattern.clone(), start, self.editor.document().generation())
            .map_err(|error| error.to_string())?;
        self.last_pattern = Some(pattern);
        self.search = Some(search);
        self.notice(format!("Searching from {:016X}; Esc cancels", start.get()));
        Ok(())
    }

    fn repeat_search(&mut self) {
        let Some(pattern) = self.last_pattern.clone() else {
            self.notice("No previous search; F7/Ctrl+F opens search");
            return;
        };
        let Some(next) = self
            .editor
            .cursor()
            .get()
            .checked_add(1)
            .filter(|next| *next < self.editor.document().len())
        else {
            self.notice("No further match before end of file; search does not wrap");
            return;
        };
        if let Err(error) = self.start_search(pattern, FileOffset::new(next)) {
            self.notice(error);
        }
    }

    fn advance_search(&mut self) {
        let Some(mut search) = self.search.take() else {
            return;
        };
        match search.step(self.editor.document_mut(), SEARCH_STEP_BYTES) {
            Ok(SearchProgress::Pending { next }) => {
                self.notice(format!("Searching {:016X}; Esc cancels", next.get()));
                self.search = Some(search);
            }
            Ok(SearchProgress::Found(offset)) => {
                self.dispatch(Command::Goto(offset));
                self.notice(format!(
                    "Found at {:016X}; Shift+F7/Ctrl+R repeats",
                    offset.get()
                ));
            }
            Ok(SearchProgress::NotFound) => {
                self.notice("No match before end of file; search does not wrap")
            }
            Err(error) => self.notice(format!("Search stopped: {error}")),
        }
    }
}

fn is_control_char(key: KeyEvent, expected: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
        && matches!(key.code, KeyCode::Char(character) if character.eq_ignore_ascii_case(&expected))
}

fn parse_hex_pattern(input: &str) -> Result<Vec<u8>, String> {
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
