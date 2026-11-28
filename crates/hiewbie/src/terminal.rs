//! Thin native terminal adapter over the portable document and command model.

mod analysis;
mod block;
mod calculator;
mod help;
mod hex_view;
mod render;
mod search;
mod symbols;
#[cfg(test)]
mod tests;
mod text_view;
#[cfg(windows)]
mod windows_cursor;

mod file_history;
mod file_picker;

use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::time::Duration;

use hiewbie::file::{BlockExport, ExportProgress, FileSource, SaveError, save_document};
use hiewbie_core::formats::{BinaryFormat, VirtualAddress};
use hiewbie_core::{
    BlockBoundary, ByteSource, Command, Document, Editor, Effect, FileNavigation, FileOffset,
    ViewMode,
};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::crossterm::{
    cursor::{Hide, SetCursorStyle, Show},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    style::ResetColor,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use analysis::{AnalysisDialog, AnalysisPage};
use block::{BlockExportRequest, BlockWrite};
use calculator::Calculator;
use file_history::{FileSession, ListKind, Selection};
use file_picker::BrowseOutcome;
use help::{Help, HelpTopic};
use search::SearchDialog;

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

enum ActiveCaret {
    Ansi,
    #[cfg(windows)]
    Native(windows_cursor::ConsoleCursor),
}

/// Each acquisition is marked before writing, because a terminal command can
/// take effect even when the subsequent flush reports an error. Drop also runs
/// during partial initialization and ordinary Rust unwinding.
struct TerminalGuard {
    raw: bool,
    alternate: bool,
    paste: bool,
    cursor_hidden: bool,
    caret: Option<ActiveCaret>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        let mut guard = Self {
            raw: true,
            alternate: false,
            paste: false,
            cursor_hidden: false,
            caret: None,
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

    fn set_input_caret(&mut self, visible: bool) -> io::Result<()> {
        if self.caret.is_some() == visible {
            return Ok(());
        }
        if !visible {
            return self.restore_caret();
        }
        #[cfg(windows)]
        if let Some(cursor) = windows_cursor::ConsoleCursor::underline() {
            self.caret = Some(ActiveCaret::Native(cursor));
            return Ok(());
        }
        // The terminal may receive the style before a flush error occurs.
        self.caret = Some(ActiveCaret::Ansi);
        execute!(io::stdout(), SetCursorStyle::SteadyUnderScore)
    }

    fn restore_caret(&mut self) -> io::Result<()> {
        match self.caret.take() {
            Some(ActiveCaret::Ansi) => {
                self.caret = Some(ActiveCaret::Ansi);
                execute!(io::stdout(), SetCursorStyle::DefaultUserShape)?;
                self.caret = None;
            }
            #[cfg(windows)]
            Some(ActiveCaret::Native(cursor)) => drop(cursor),
            None => {}
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Continue restoring independent modes after an individual I/O failure.
        if self.alternate {
            let _ = execute!(io::stdout(), ResetColor);
        }
        let _ = self.restore_caret();
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

pub(crate) fn run(
    start: Option<(Editor<FileSource>, &Path)>,
    read_only: bool,
    additional_paths: Vec<PathBuf>,
) -> io::Result<()> {
    let mut guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;
    // Both buffers are empty. Clear the alternate screen without querying
    // cursor position from terminals that cannot answer the report request.
    terminal.backend_mut().clear()?;
    let mut picker = None;
    let first_argument = start.as_ref().map(|(_, path)| *path);
    let mut ui = match start {
        Some((editor, path)) => Ui::new(editor, path.to_string_lossy().into_owned()),
        None => match file_picker::browse(&mut terminal, &mut guard, &mut picker, None)? {
            BrowseOutcome::Open(source) => ui_for_picked_file(source, read_only),
            BrowseOutcome::Close | BrowseOutcome::History => return Ok(()),
        },
    };
    let mut files = FileSession::new(&ui, first_argument, additional_paths, read_only);
    let mut redraw = true;
    let mut export: Option<BlockExport> = None;

    loop {
        if redraw {
            guard.set_input_caret(ui.has_input_caret())?;
            terminal.draw(|frame| ui.render(frame))?;
        }
        redraw = false;
        let timeout = if ui.editor.is_searching() || export.is_some() {
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
                UiAction::BrowseFiles => {
                    choose_file(
                        &mut terminal,
                        &mut guard,
                        &mut picker,
                        &mut ui,
                        &mut files,
                        None,
                    )?;
                }
                UiAction::NavigateFile(navigation) => {
                    let list = match navigation {
                        FileNavigation::History => Some(ListKind::Recent),
                        FileNavigation::Arguments => Some(ListKind::Arguments),
                        _ => None,
                    };
                    if let Some(list) = list {
                        choose_file(
                            &mut terminal,
                            &mut guard,
                            &mut picker,
                            &mut ui,
                            &mut files,
                            Some(list),
                        )?;
                    } else {
                        files.advance(&mut ui, navigation);
                    }
                }
                UiAction::Save => match save_document(ui.editor.document_mut()) {
                    Ok(source) => {
                        ui.editor.complete_save(source);
                        ui.file_access = FileAccess::Updated;
                        ui.pending_nibble = None;
                        ui.hint("Saved; edit transaction completed");
                    }
                    Err(error) => ui.save_failed(error),
                },
                UiAction::ExportBlock(request) => match BlockExport::begin(
                    ui.editor.document(),
                    request.range,
                    &request.path,
                    request.offset,
                    request.replace_existing,
                ) {
                    Ok(Some(operation)) => {
                        export = Some(operation);
                        ui.export_in_progress = true;
                        ui.notice("Staging block export | Esc cancels");
                    }
                    Ok(None) => ui.confirm_block_output(),
                    Err(error) => ui.report_export_failure(error.to_string()),
                },
                UiAction::CancelBlockExport => {
                    export = None;
                    ui.report_export_failure("Block export canceled");
                }
                UiAction::Continue => {}
            }
        }
        if let Some(operation) = &mut export {
            redraw = true;
            match operation.step(ui.editor.document_mut()) {
                Ok(ExportProgress::Staging { done, total }) => {
                    ui.notice(format!(
                        "Staging block output {done}/{total} bytes | Esc cancels"
                    ));
                }
                Ok(ExportProgress::Complete { block_bytes }) => {
                    export = None;
                    ui.export_in_progress = false;
                    ui.dialog = None;
                    ui.notice(format!("Exported {block_bytes} block bytes"));
                }
                Err(error) => {
                    export = None;
                    ui.report_export_failure(error.to_string());
                }
            }
        }
        if ui.editor.is_searching() {
            ui.advance_search();
            redraw = true;
        }
    }
    Ok(())
}

fn choose_file(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    guard: &mut TerminalGuard,
    picker: &mut Option<file_picker::FilePicker>,
    ui: &mut Ui<FileSource>,
    files: &mut FileSession,
    mut list: Option<ListKind>,
) -> io::Result<()> {
    loop {
        list = if let Some(kind) = list {
            match files.pick(terminal, guard, ui, kind)? {
                Selection::Close => return Ok(()),
                Selection::Files => None,
            }
        } else {
            match file_picker::browse(terminal, guard, picker, Some(ui))? {
                BrowseOutcome::Close => return Ok(()),
                BrowseOutcome::History => Some(ListKind::Recent),
                BrowseOutcome::Open(source) => {
                    files.open_source(ui, source);
                    return Ok(());
                }
            }
        };
    }
}

fn ui_for_picked_file(source: FileSource, read_only: bool) -> Ui<FileSource> {
    let label = source.path().to_string_lossy().into_owned();
    let mut ui = Ui::new(Editor::new(Document::new(source), read_only), label);
    ui.dispatch(Command::SetMode(ViewMode::Text));
    ui
}

#[derive(Debug, Eq, PartialEq)]
enum UiAction {
    Continue,
    BrowseFiles,
    NavigateFile(FileNavigation),
    Save,
    ExportBlock(BlockExportRequest),
    CancelBlockExport,
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
    RawBase,
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
    Search(SearchDialog),
    SearchNotice(&'static str),
    BlockWrite(BlockWrite),
    Calculator,
    Help(Help),
    Mode(ModeChoice),
    LineFeed(text_view::LineFeed),
    Analysis(AnalysisDialog),
}

struct Ui<S: ByteSource> {
    editor: Editor<S>,
    label: String,
    short_label: String,
    top: u64,
    bytes_per_row: usize,
    visible_rows: usize,
    text: text_view::TextView,
    entry_column: EntryColumn,
    pending_nibble: Option<u8>,
    message: String,
    show_hex_message: bool,
    file_access: FileAccess,
    edit_format: Option<BinaryFormat>,
    calculator: Calculator,
    dialog: Option<Dialog>,
    reopen_required: bool,
    export_in_progress: bool,
    scan_position: Option<FileOffset>,
}

#[derive(Clone, Copy)]
enum FileAccess {
    Read,
    Write,
    Updated,
}

impl<S: ByteSource> Ui<S> {
    fn new(mut editor: Editor<S>, label: String) -> Self {
        editor.refresh_analysis();
        let short_label = Path::new(&label)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&label);
        Self {
            editor,
            label: escaped_text(&label),
            short_label: escaped_text(short_label),
            top: 0,
            bytes_per_row: 16,
            visible_rows: 21,
            text: text_view::TextView::default(),
            entry_column: EntryColumn::Hex,
            pending_nibble: None,
            message: "F1 help | Enter cycle | F4 select mode | F8 headers | Esc quit".into(),
            show_hex_message: false,
            file_access: FileAccess::Read,
            edit_format: None,
            calculator: Calculator::default(),
            dialog: None,
            reopen_required: false,
            export_in_progress: false,
            scan_position: None,
        }
    }

    fn notice(&mut self, text: impl AsRef<str>) {
        self.message = escaped_text(text.as_ref());
        self.show_hex_message = true;
    }

    fn hint(&mut self, text: impl AsRef<str>) {
        self.message = escaped_text(text.as_ref());
        self.show_hex_message = false;
    }

    fn dispatch(&mut self, command: Command) -> UiAction {
        if command != Command::OpenCalculator && self.pending_nibble.take().is_some() {
            self.notice("Pending hex digit cancelled; no partial byte was applied");
        }
        match self.editor.dispatch(command) {
            Ok(Effect::None) => UiAction::Continue,
            Ok(Effect::BrowseFiles) => UiAction::BrowseFiles,
            Ok(Effect::FileNavigation(navigation)) => UiAction::NavigateFile(navigation),
            Ok(Effect::Save) => UiAction::Save,
            Ok(Effect::ExportBlock(range)) => {
                self.dialog = Some(Dialog::BlockWrite(BlockWrite::new(range)));
                UiAction::Continue
            }
            Ok(Effect::OpenCalculator) => {
                self.calculator.reopen();
                self.dialog = Some(Dialog::Calculator);
                UiAction::Continue
            }
            Err(error) => {
                self.notice(error.to_string());
                UiAction::Continue
            }
        }
    }

    fn reject_paste(&mut self) {
        if !matches!(self.dialog, Some(Dialog::Calculator)) {
            self.pending_nibble = None;
        }
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
        self.show_hex_message = false;
        if self.editor.is_searching() {
            if key.code == KeyCode::Esc || is_control_char(key, 'c') {
                self.editor.cancel_search();
                self.scan_position = None;
                self.hint("Search canceled; cursor unchanged");
                self.dialog = Some(Dialog::SearchNotice("Aborted by hacker"));
            }
            return UiAction::Continue;
        }
        if self.export_in_progress {
            return if key.code == KeyCode::Esc || is_control_char(key, 'c') {
                UiAction::CancelBlockExport
            } else {
                UiAction::Continue
            };
        }
        if self.dialog.is_some() {
            return self.dialog_key(key);
        }
        if matches!(
            (key.code, key.modifiers),
            (KeyCode::Char('='), KeyModifiers::ALT)
        ) || is_control_char(key, 'l')
        {
            return self.dispatch(Command::OpenCalculator);
        }
        if key.code == KeyCode::F(1)
            && key.modifiers == KeyModifiers::SHIFT
            && !self.editor.is_editing()
        {
            self.open_analysis(AnalysisPage::Profiles);
            return UiAction::Continue;
        }
        if !self.editor.is_editing() && self.editor.mode() != ViewMode::Text {
            let block_command = match (key.code, key.modifiers) {
                (KeyCode::Char('*'), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    // Enhanced keyboard protocols can report auto-repeat separately.
                    if key.kind == KeyEventKind::Repeat {
                        return UiAction::Continue;
                    }
                    Some(Command::ToggleBlock)
                }
                (KeyCode::Char('['), KeyModifiers::NONE) => {
                    Some(Command::GotoBlockBoundary(BlockBoundary::First))
                }
                (KeyCode::Char(']'), KeyModifiers::NONE) => {
                    Some(Command::GotoBlockBoundary(BlockBoundary::Last))
                }
                (KeyCode::Char('*'), mods) if mods - KeyModifiers::SHIFT == KeyModifiers::ALT => {
                    Some(Command::ResizeBlock)
                }
                (KeyCode::Char('*' | 'k' | 'K'), mods)
                    if mods - KeyModifiers::SHIFT == KeyModifiers::CONTROL =>
                {
                    Some(Command::SelectWholeBlock)
                }
                _ => None,
            };
            if let Some(command) = block_command {
                let previous = self.editor.block_selection();
                let action = self.dispatch(command);
                if self.editor.block_selection() != previous {
                    self.notice(match self.editor.block_selection() {
                        Some(block) if block.is_marking => "Marking block | * finish | Esc cancel",
                        Some(_) => "Block marked | [ first | ] last | * clear",
                        None => "Block cleared",
                    });
                }
                return action;
            }
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

        if self.text_navigation()
            && let Some(action) = self.text_key(key.code)
        {
            return action;
        }

        match key.code {
            KeyCode::Esc => self.cancel_or_exit(),
            KeyCode::F(1) => {
                let topic = if self.editor.is_editing() {
                    HelpTopic::Edit(self.editor.mode())
                } else {
                    HelpTopic::View(self.editor.mode())
                };
                self.dialog = Some(Dialog::Help(Help::new(topic)));
                UiAction::Continue
            }
            KeyCode::Left => self.dispatch(Command::Move(-1)),
            KeyCode::Right => self.dispatch(Command::Move(1)),
            KeyCode::Up if self.code_navigation() => self.dispatch(Command::MoveCode(-1)),
            KeyCode::Down if self.code_navigation() => self.dispatch(Command::MoveCode(1)),
            KeyCode::PageUp if self.code_navigation() => {
                self.dispatch(Command::MoveCode(-(self.visible_rows as i32)))
            }
            KeyCode::PageDown if self.code_navigation() => {
                self.dispatch(Command::MoveCode(self.visible_rows as i32))
            }
            KeyCode::Home if self.code_navigation() => self.dispatch(Command::ResyncCode),
            KeyCode::End if self.code_navigation() => {
                self.dispatch(Command::MoveCode(self.visible_rows.saturating_sub(1) as i32))
            }
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

    fn has_input_caret(&self) -> bool {
        if self.reopen_required || self.export_in_progress || self.editor.is_searching() {
            return false;
        }
        match self.dialog.as_ref() {
            Some(Dialog::Input(_)) => true,
            Some(Dialog::Search(search)) => search.help.is_none(),
            Some(Dialog::Calculator) => self.calculator.help.is_none(),
            Some(Dialog::BlockWrite(dialog)) => dialog.focus != block::BlockField::Format,
            None => self.editor.is_editing() && !self.editor.document().is_empty(),
            _ => false,
        }
    }

    fn editing_caret_visible(&self) -> bool {
        self.editor.is_editing() && self.dialog.is_none()
    }

    fn page_step(&self) -> i64 {
        (self.visible_rows.max(1) * self.bytes_per_row) as i64
    }

    fn code_navigation(&self) -> bool {
        self.editor.mode() == ViewMode::Code && !self.editor.is_editing()
    }

    fn cancel_or_exit(&mut self) -> UiAction {
        if self.editor.is_editing() {
            let action = self.dispatch(Command::DiscardEdit);
            self.hint("Pending edits discarded; viewing original bytes. Esc quits");
            action
        } else if self
            .editor
            .block_selection()
            .is_some_and(|block| block.is_marking)
        {
            let action = self.dispatch(Command::ClearBlock);
            self.notice("Block marking canceled");
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
            ViewMode::Hex | ViewMode::Code => EntryColumn::Hex,
            ViewMode::Text => EntryColumn::Ascii,
        };
        self.editor.refresh_analysis();
        self.edit_format = self.editor.executable().map(|image| image.format);
        self.hint("Editing: F9 saves; Esc discards; F3 restores byte; Backspace restores previous");
        let action = self.dispatch(Command::BeginEdit);
        if self.editor.is_editing() {
            self.file_access = FileAccess::Write;
        }
        action
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
            KeyCode::F(2) if self.editor.mode() != ViewMode::Text => {
                self.dispatch(Command::ExportBlock)
            }
            KeyCode::Enter => {
                self.hint("Mode changed; Shift+F1/Ctrl+A selects the Code architecture");
                self.dispatch(Command::CycleMode)
            }
            KeyCode::F(4) => {
                self.dialog = Some(Dialog::Mode(match self.editor.mode() {
                    ViewMode::Text => ModeChoice::Text,
                    ViewMode::Hex => ModeChoice::Hex,
                    ViewMode::Code => ModeChoice::Code,
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
            KeyCode::F(8) => {
                self.open_analysis(AnalysisPage::Overview);
                UiAction::Continue
            }
            KeyCode::Backspace if self.code_navigation() => self.dispatch(Command::ReturnBranch),
            KeyCode::Char('/') if self.code_navigation() => self.dispatch(Command::ResyncCode),
            KeyCode::Char(number @ '1'..='9') if self.code_navigation() => {
                self.dispatch(Command::FollowBranch((number as u8 - b'0') as usize))
            }
            KeyCode::F(9) => self.dispatch(Command::BrowseFiles),
            KeyCode::Tab => self.dispatch(Command::FileNavigation(FileNavigation::NextRecent)),
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
                self.hint(match self.entry_column {
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
                self.hint("Pending hex digit cancelled; document unchanged");
                UiAction::Continue
            }
            KeyCode::Backspace => {
                self.hint(if self.editor.cursor().get() == 0 {
                    "No previous byte at start of file"
                } else {
                    "Previous byte restored; F9 saves; Esc discards"
                });
                self.dispatch(Command::RestorePreviousByte)
            }
            KeyCode::Char(character) => self.enter_character(character),
            KeyCode::F(3) => {
                self.pending_nibble = None;
                self.hint("Original byte restored; F9 saves; Esc discards");
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
                    self.hint("Byte overwrite applied; F9/Ctrl+S saves; Esc discards");
                    self.dispatch(Command::WriteByte((high << 4) | digit as u8))
                } else {
                    self.pending_nibble = Some(digit as u8);
                    self.hint(format!(
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
                    self.hint("Byte overwrite applied; F9/Ctrl+S saves; Esc discards");
                    self.dispatch(Command::WriteByte(character as u8))
                }
            }
        }
    }

    fn control_key(&mut self, key: KeyEvent) -> UiAction {
        if key.modifiers == KeyModifiers::CONTROL && !self.editor.is_editing() {
            let navigation = match key.code {
                KeyCode::Backspace | KeyCode::Char('h') => Some(FileNavigation::History),
                KeyCode::F(9) => Some(FileNavigation::Arguments),
                KeyCode::F(11) => Some(FileNavigation::PreviousArgument),
                KeyCode::F(12) => Some(FileNavigation::NextArgument),
                _ => None,
            };
            if let Some(navigation) = navigation {
                return self.dispatch(Command::FileNavigation(navigation));
            }
        }
        if key.modifiers.intersects(
            KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::HYPER | KeyModifiers::META,
        ) {
            self.pending_nibble = None;
            self.notice("This modifier combination is not implemented");
            return UiAction::Continue;
        }
        match key.code {
            KeyCode::F(1)
                if key.modifiers == KeyModifiers::CONTROL && !self.editor.is_editing() =>
            {
                self.cycle_bitness()
            }
            KeyCode::F(6) if key.modifiers == KeyModifiers::CONTROL && self.text_navigation() => {
                self.toggle_text_tabs()
            }
            KeyCode::Char('b' | 'B') if !self.editor.is_editing() => self.cycle_bitness(),
            KeyCode::Char('a' | 'A') if !self.editor.is_editing() => {
                self.open_analysis(AnalysisPage::Profiles);
                UiAction::Continue
            }
            KeyCode::Char('o' | 'O') if !self.editor.is_editing() => {
                self.open_analysis(AnalysisPage::Overview);
                UiAction::Continue
            }
            KeyCode::Home | KeyCode::End => {
                let offset = if key.code == KeyCode::Home {
                    0
                } else {
                    self.editor.document().len().saturating_sub(1)
                };
                let action = self.dispatch(Command::Goto(FileOffset::new(offset)));
                self.editor.reset_search_origin();
                action
            }
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

    fn dialog_key(&mut self, key: KeyEvent) -> UiAction {
        let Some(dialog) = self.dialog.take() else {
            return UiAction::Continue;
        };
        let mut input = match dialog {
            Dialog::Calculator => {
                self.calculator_key(key);
                return UiAction::Continue;
            }
            Dialog::BlockWrite(dialog) => return self.block_write_key(dialog, key),
            Dialog::Search(search) => {
                self.search_key(search, key);
                return UiAction::Continue;
            }
            _ if key.code == KeyCode::Esc || is_control_char(key, 'c') => {
                self.hint("Dialog canceled");
                return UiAction::Continue;
            }
            Dialog::SearchNotice(message) => {
                if key.code != KeyCode::Enter || !key.modifiers.is_empty() {
                    self.dialog = Some(Dialog::SearchNotice(message));
                }
                return UiAction::Continue;
            }
            Dialog::Input(input) => input,
            Dialog::Analysis(dialog) => {
                self.analysis_key(dialog, key);
                return UiAction::Continue;
            }
            Dialog::Help(mut help) => {
                if !help.closes(key) {
                    self.dialog = Some(Dialog::Help(help));
                }
                return UiAction::Continue;
            }
            Dialog::LineFeed(selected) => {
                self.text_line_feed_key(selected, key);
                return UiAction::Continue;
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
                    if key.code == KeyCode::F(4) {
                        self.dispatch(Command::CycleMode);
                        self.hint(format!("{} view selected", self.editor.mode()));
                        return UiAction::Continue;
                    }
                    let mode = match key.code {
                        KeyCode::F(1) => Some(ViewMode::Text),
                        KeyCode::F(2) => Some(ViewMode::Hex),
                        KeyCode::F(3) => Some(ViewMode::Code),
                        KeyCode::Enter => Some(match selected {
                            ModeChoice::Text => ViewMode::Text,
                            ModeChoice::Hex => ViewMode::Hex,
                            ModeChoice::Code => ViewMode::Code,
                        }),
                        _ => None,
                    };
                    if let Some(mode) = mode {
                        self.dispatch(Command::SetMode(mode));
                        self.hint(format!("{mode} view selected; cursor unchanged"));
                        return UiAction::Continue;
                    }
                }
                self.dialog = Some(Dialog::Mode(selected));
                return UiAction::Continue;
            }
        };
        match key.code {
            KeyCode::Enter => match self.apply_prompt(&input) {
                Ok(()) => return UiAction::Continue,
                Err(error) => input.error = Some(escaped_text(&error)),
            },
            KeyCode::Backspace => {
                input.input.pop();
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
                if input.input.len() + character.len_utf8() <= 64 {
                    input.input.push(character);
                    input.error = None;
                } else {
                    input.error = Some("Input limit reached".into());
                }
            }
            _ => {}
        }
        self.dialog = Some(Dialog::Input(input));
        UiAction::Continue
    }

    fn apply_prompt(&mut self, prompt: &InputDialog) -> Result<(), String> {
        match prompt.kind {
            PromptKind::Goto => {
                self.editor
                    .goto_expression(&prompt.input)
                    .map_err(|error| error.to_string())?;
                self.hint(format!("File offset {:016X}", self.editor.cursor().get()));
                Ok(())
            }
            PromptKind::RawBase => {
                let base = analysis::parse_base(&prompt.input)?;
                self.editor
                    .dispatch(Command::SetRawBase(VirtualAddress::new(base)))
                    .map_err(|error| error.to_string())?;
                self.notice(format!(
                    "Raw address base {base:016X}; decoder selection is independent"
                ));
                Ok(())
            }
        }
    }
}

fn is_control_char(key: KeyEvent, expected: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
        && matches!(key.code, KeyCode::Char(character) if character.eq_ignore_ascii_case(&expected))
}
