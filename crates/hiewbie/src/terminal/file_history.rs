use std::io::{self, Stdout};
use std::path::{Path, PathBuf};

use hiewbie::file::FileSource;
use hiewbie_core::{Command, FileNavigation, FileOffset, ViewMode};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use super::help::{Help, HelpTopic};
use super::render::{
    FOCUSED_ROW, HEADER, dialog_surface, dialog_surface_with_tinted_shadow,
    render_full_function_bar,
};
use super::{TerminalGuard, Ui, escaped_text, is_control_char, ui_for_picked_file};

const MAX_RECENT: usize = 64;
const BODY: Style = HEADER.fg(Color::Blue);
const ERROR: Style = Style::new().fg(Color::LightYellow).bg(Color::Red);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Position {
    mode: ViewMode,
    offset: FileOffset,
    top: u64,
}

impl Position {
    fn save(ui: &Ui<FileSource>) -> Self {
        Self {
            mode: ui.editor.mode(),
            offset: ui.editor.cursor(),
            top: ui.top,
        }
    }

    fn restore(self, ui: &mut Ui<FileSource>) {
        ui.dispatch(Command::SetMode(self.mode));
        ui.dispatch(Command::Goto(self.offset));
        ui.top = self.top.min(ui.editor.cursor().get());
    }
}

#[derive(Debug)]
struct Visit {
    path: PathBuf,
    position: Position,
    marked: bool,
}

struct Argument {
    path: PathBuf,
    label: String,
}

impl Argument {
    fn new(path: &Path) -> Self {
        Self {
            path: std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()),
            label: escaped_text(&path.to_string_lossy()),
        }
    }
}

pub(super) struct FileSession {
    recent: Vec<Visit>,
    arguments: Vec<Argument>,
    argument_index: Option<usize>,
    argument_width: u16,
    read_only: bool,
    full_paths: bool,
}

impl FileSession {
    pub(super) fn new(
        ui: &Ui<FileSource>,
        first_argument: Option<&Path>,
        additional: Vec<PathBuf>,
        read_only: bool,
    ) -> Self {
        let current = active_path(ui).to_path_buf();
        let arguments: Vec<_> = first_argument
            .into_iter()
            .chain(additional.iter().map(PathBuf::as_path))
            .map(Argument::new)
            .collect();
        let argument_width = arguments.iter().fold(12, |width, entry| {
            width.max(Line::raw(entry.label.as_str()).width())
        });
        Self {
            recent: vec![Visit {
                path: current,
                position: Position::save(ui),
                marked: false,
            }],
            argument_index: (!arguments.is_empty()).then_some(0),
            argument_width: u16::try_from(argument_width).unwrap_or(u16::MAX),
            arguments,
            read_only,
            full_paths: true,
        }
    }

    fn visit_index(&self, path: &Path) -> Option<usize> {
        self.recent.iter().position(|entry| entry.path == path)
    }

    fn update(&mut self, ui: &Ui<FileSource>) {
        if let Some(index) = self.visit_index(active_path(ui)) {
            self.recent[index].position = Position::save(ui);
        }
    }

    pub(super) fn open_source(&mut self, ui: &mut Ui<FileSource>, source: FileSource) {
        self.switch(ui, source, true);
    }

    fn switch(&mut self, ui: &mut Ui<FileSource>, source: FileSource, promote: bool) {
        let path = source.path().to_path_buf();
        self.update(ui);
        if path != active_path(ui) {
            self.argument_index = self
                .arguments
                .iter()
                .position(|argument| argument.path == path);
        }
        let previous = self.visit_index(&path);
        let mut next = ui_for_picked_file(source, self.read_only);
        if let Some(index) = previous {
            self.recent[index].position.restore(&mut next);
            if promote {
                let visit = self.recent.remove(index);
                self.recent.insert(0, visit);
            }
        } else {
            self.recent.insert(
                0,
                Visit {
                    path,
                    position: Position::save(&next),
                    marked: false,
                },
            );
            self.recent.truncate(MAX_RECENT);
        }
        *ui = next;
    }

    pub(super) fn advance(&mut self, ui: &mut Ui<FileSource>, navigation: FileNavigation) {
        let current = active_path(ui);
        let argument = match navigation {
            FileNavigation::PreviousArgument => self.adjacent_argument(false),
            FileNavigation::NextArgument => self.adjacent_argument(true),
            _ => None,
        };
        let path = match navigation {
            FileNavigation::NextRecent => self.next_recent(current),
            FileNavigation::PreviousArgument | FileNavigation::NextArgument => {
                argument.map(|index| self.arguments[index].path.as_path())
            }
            _ => return,
        };
        let Some(path) = path else {
            return;
        };
        if path == current {
            if argument.is_some() {
                self.argument_index = argument;
            }
            return;
        }
        match FileSource::open(path) {
            Ok(source) => {
                self.switch(ui, source, navigation != FileNavigation::NextRecent);
                if argument.is_some() {
                    self.argument_index = argument;
                }
            }
            Err(error) => ui.notice(format!("Cannot open {}: {error}", path.display())),
        }
    }

    fn next_recent(&self, path: &Path) -> Option<&Path> {
        let from = self.visit_index(path).map_or(0, |index| index + 1);
        let marked_only = self.recent.iter().any(|visit| visit.marked);
        self.recent
            .iter()
            .cycle()
            .skip(from)
            .take(self.recent.len())
            .find(|entry| !marked_only || entry.marked)
            .map(|entry| entry.path.as_path())
    }

    fn adjacent_argument(&self, forward: bool) -> Option<usize> {
        let count = self.arguments.len();
        if count == 0 {
            return None;
        }
        let index = match (self.argument_index, forward) {
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => index.checked_sub(1).unwrap_or(count - 1),
            (None, true) => 0,
            (None, false) => count - 1,
        };
        Some(index)
    }

    fn row_count(&self, kind: ListKind) -> usize {
        if kind == ListKind::Recent {
            self.recent.len()
        } else {
            self.arguments.len()
        }
    }

    fn row_path(&self, kind: ListKind, index: usize) -> &Path {
        match kind {
            ListKind::Recent => &self.recent[index].path,
            ListKind::Arguments => &self.arguments[index].path,
        }
    }

    pub(super) fn pick(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        guard: &mut TerminalGuard,
        ui: &mut Ui<FileSource>,
        kind: ListKind,
    ) -> io::Result<Selection> {
        self.update(ui);
        let selected = match kind {
            ListKind::Recent => self.visit_index(active_path(ui)),
            ListKind::Arguments => self.argument_index,
        };
        let current = active_path(ui).to_path_buf();
        let mut dialog = List::new(kind, selected.unwrap_or(0));
        guard.set_input_caret(false)?;
        loop {
            terminal.draw(|frame| {
                ui.render(frame);
                dialog.render(frame, self);
            })?;
            if let Event::Key(key) = event::read()? {
                match dialog.key(self, &current, key) {
                    ListAction::Continue => {}
                    ListAction::Close => return Ok(Selection::Close),
                    ListAction::Files => return Ok(Selection::Files),
                    ListAction::Open(index) => {
                        let path = self.row_path(kind, index);
                        if path == current {
                            if kind == ListKind::Arguments {
                                self.argument_index = Some(index);
                            }
                            return Ok(Selection::Close);
                        }
                        match FileSource::open(path) {
                            Ok(source) => {
                                self.open_source(ui, source);
                                if kind == ListKind::Arguments {
                                    self.argument_index = Some(index);
                                }
                                return Ok(Selection::Close);
                            }
                            Err(error) => {
                                dialog.error = Some(escaped_text(&format!(
                                    "Cannot open {}: {error}",
                                    path.display()
                                )));
                            }
                        }
                    }
                }
            }
        }
    }
}

fn active_path(ui: &Ui<FileSource>) -> &Path {
    ui.editor.document().source().path()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ListKind {
    Recent,
    Arguments,
}

pub(super) enum Selection {
    Close,
    Files,
}

enum ListAction {
    Continue,
    Close,
    Files,
    Open(usize),
}

struct List {
    kind: ListKind,
    selected: usize,
    first: usize,
    page: usize,
    help: Option<Help>,
    error: Option<String>,
}

impl List {
    fn new(kind: ListKind, selected: usize) -> Self {
        Self {
            kind,
            selected,
            first: 0,
            page: 1,
            help: None,
            error: None,
        }
    }

    fn key(&mut self, files: &mut FileSession, current: &Path, key: KeyEvent) -> ListAction {
        if key.kind == KeyEventKind::Release {
            return ListAction::Continue;
        }
        if let Some(help) = &mut self.help {
            if help.closes(key) {
                self.help = None;
            }
            return ListAction::Continue;
        }
        if self.error.take().is_some() {
            return ListAction::Continue;
        }
        if is_control_char(key, 'c') || key.code == KeyCode::Esc && key.modifiers.is_empty() {
            if self.kind == ListKind::Arguments && files.row_count(self.kind) != 0 {
                files.argument_index = Some(self.selected);
            }
            return ListAction::Close;
        }
        let recent = self.kind == ListKind::Recent;
        if recent && key.code == KeyCode::F(5) {
            let marked = match key.modifiers {
                KeyModifiers::CONTROL => Some(true),
                KeyModifiers::ALT => Some(false),
                _ => None,
            };
            if let Some(marked) = marked {
                files
                    .recent
                    .iter_mut()
                    .for_each(|entry| entry.marked = marked);
                return ListAction::Continue;
            }
        }
        if !key.modifiers.is_empty() {
            return ListAction::Continue;
        }
        let count = files.row_count(self.kind);
        let last = count.saturating_sub(1);
        match key.code {
            KeyCode::Enter if count > 0 => return ListAction::Open(self.selected),
            KeyCode::F(9) if recent => return ListAction::Files,
            KeyCode::F(1) if recent => self.help = Some(Help::new(HelpTopic::HistoryFiles)),
            KeyCode::F(2) if recent => files.full_paths = !files.full_paths,
            KeyCode::F(5) if recent && count > 0 => {
                let entry = &mut files.recent[self.selected];
                entry.marked = !entry.marked;
            }
            KeyCode::F(8) if recent && count > 0 && files.recent[self.selected].path != current => {
                files.recent.remove(self.selected);
                self.selected = self.selected.min(count.saturating_sub(2));
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(last),
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(self.page),
            KeyCode::PageDown => self.selected = (self.selected + self.page).min(last),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = last,
            _ => {}
        }
        ListAction::Continue
    }

    fn render(&mut self, frame: &mut Frame<'_>, files: &FileSession) {
        let area = frame.area();
        if area.is_empty() {
            return;
        }
        let centered = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(1),
        );
        let height = area.height.saturating_sub(4).max(5);
        let content = if self.kind == ListKind::Recent {
            dialog_surface_with_tinted_shadow(
                frame,
                centered,
                area.width.saturating_sub(8).max(12),
                height,
                Line::raw("═ Mode  Offset "),
                HEADER,
            )
        } else {
            dialog_surface_with_tinted_shadow(
                frame,
                centered,
                files.argument_width.saturating_add(8),
                height,
                Line::default(),
                HEADER,
            )
        };
        self.page = usize::from(content.height).max(1);
        let count = files.row_count(self.kind);
        self.first = self
            .first
            .clamp(self.selected.saturating_sub(self.page - 1), self.selected)
            .min(count.saturating_sub(self.page));
        let rows = content.inner(Margin::new(1, 0));
        if !rows.is_empty() {
            for (relative, index) in (self.first..count).take(self.page).enumerate() {
                let line = Rect::new(rows.x, rows.y + relative as u16, rows.width, 1);
                let style = if self.selected == index {
                    FOCUSED_ROW
                } else {
                    BODY
                };
                frame.render_widget(Block::default().style(style), line);
                let row = if self.kind == ListKind::Recent {
                    history_row(&files.recent[index], files.full_paths)
                } else {
                    files.arguments[index].label.as_str().into()
                };
                frame.render_widget(Paragraph::new(row).style(style), line);
            }
        }
        if self.kind == ListKind::Arguments && !rows.is_empty() {
            let label = format!("{count} files");
            let width = u16::try_from(label.len())
                .unwrap_or(u16::MAX)
                .min(rows.width);
            frame.render_widget(
                Paragraph::new(label).style(HEADER),
                Rect::new(rows.x + (rows.width - width) / 2, rows.bottom(), width, 1),
            );
        }
        let bar = Rect::new(area.x, area.bottom() - 1, area.width, 1);
        if let Some(help) = &mut self.help {
            render_full_function_bar(frame, bar, &[""; 12], &[]);
            help.render(frame, area);
        } else if self.kind == ListKind::Recent {
            render_full_function_bar(
                frame,
                bar,
                &[
                    "Help", "Path", "", "", "Mark", "", "", "Delete", "Files", "", "", "",
                ],
                &[],
            );
        } else {
            render_full_function_bar(frame, bar, &[""; 12], &[]);
        }
        if let Some(error) = &self.error {
            let content = dialog_surface(frame, area, 66, 7, Line::default(), ERROR);
            frame.render_widget(
                Paragraph::new(error.as_str())
                    .wrap(Wrap { trim: true })
                    .style(ERROR),
                content,
            );
        }
    }
}

fn history_row(entry: &Visit, full_path: bool) -> String {
    let path = if full_path {
        entry.path.as_os_str()
    } else {
        entry.path.file_name().unwrap_or(entry.path.as_os_str())
    };
    let path = escaped_text(&path.to_string_lossy());
    let mode = match entry.position.mode {
        ViewMode::Text => "Text",
        ViewMode::Hex => "Hex ",
        ViewMode::Code => "Code",
    };
    let mark = if entry.marked { '•' } else { ' ' };
    format!("{mark}{mode}│{:08X}│{path}", entry.position.offset.get())
}

#[cfg(test)]
#[path = "file_history/tests.rs"]
mod tests;
