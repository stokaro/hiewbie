mod filter;
mod timestamp;

use std::ffi::{OsStr, OsString};
use std::fs::{self, Metadata};
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use hiewbie::file::FileSource;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::{Frame, Terminal};

use super::help::{Help, HelpTopic};
use super::render::{BASE, FOCUSED_ROW, HEADER, dialog_surface, render_full_function_bar};
use super::{TerminalGuard, Ui, escaped_text, is_control_char};
use filter::FilterInput;

const MAX_ENTRIES: usize = 16_384;
const BODY: Style = HEADER.fg(Color::Blue);
const ERROR: Style = Style::new().fg(Color::LightYellow).bg(Color::Red);

pub(super) fn browse(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    guard: &mut TerminalGuard,
    state: &mut Option<FilePicker>,
    mut editor: Option<&mut Ui<FileSource>>,
) -> io::Result<Option<FileSource>> {
    if state.is_none() {
        let current_file = editor
            .as_ref()
            .map(|ui| ui.editor.document().source().path());
        let directory = match std::env::current_dir() {
            Ok(directory) => directory,
            Err(error) => {
                if let Some(ui) = editor {
                    ui.notice(format!("Cannot read the working directory: {error}"));
                    return Ok(None);
                }
                return Err(error);
            }
        };
        *state = Some(FilePicker::new(directory, current_file));
    }
    let picker = state.as_mut().expect("the browser has been initialized");
    picker.reload();
    loop {
        guard.set_input_caret(picker.filter_input.is_some() && picker.help.is_none())?;
        terminal.draw(|frame| {
            if let Some(ui) = editor.as_mut() {
                ui.render(frame);
            } else {
                frame.render_widget(Block::default().style(BASE), frame.area());
            }
            picker.render(frame);
        })?;
        match event::read()? {
            Event::Key(key) => match picker.key(key) {
                Action::Continue => {}
                Action::Close => return Ok(None),
                Action::Open(path) => {
                    if editor
                        .as_ref()
                        .is_some_and(|ui| ui.editor.document().source().path() == path)
                    {
                        return Ok(None);
                    }
                    match FileSource::open(&path) {
                        Ok(source) => return Ok(Some(source)),
                        Err(error) => picker.report_error(&path, &error),
                    }
                }
            },
            Event::Resize(_, _) => {}
            _ => continue,
        }
    }
}

enum Action {
    Continue,
    Close,
    Open(PathBuf),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Extension,
    Time,
    Size,
    DirectoryOrder,
}

struct Entry {
    name: OsString,
    metadata: Option<Metadata>,
    display: String,
    screen_width: u16,
    name_key: String,
    extension_key: String,
    modified: Option<SystemTime>,
}

impl Entry {
    fn new(name: OsString, metadata: Option<Metadata>) -> Self {
        let display = escaped_text(&name.to_string_lossy());
        let screen_width = u16::try_from(Line::raw(display.as_str()).width()).unwrap_or(u16::MAX);
        let name_key = name.to_string_lossy().to_lowercase();
        let extension_key = Path::new(&name)
            .extension()
            .map_or_else(String::new, |value| value.to_string_lossy().to_lowercase());
        let modified = metadata
            .as_ref()
            .and_then(|metadata| metadata.modified().ok());
        Self {
            name,
            metadata,
            display,
            screen_width,
            name_key,
            extension_key,
            modified,
        }
    }

    fn directory(&self) -> bool {
        self.metadata.as_ref().is_none_or(Metadata::is_dir)
    }

    fn size(&self) -> u64 {
        self.metadata.as_ref().map_or(0, Metadata::len)
    }

    fn attributes(&self) -> [char; 4] {
        let Some(metadata) = &self.metadata else {
            return ['.'; 4];
        };
        #[cfg(windows)]
        let flags = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes()
        };
        #[cfg(unix)]
        let flags = u32::from(metadata.permissions().readonly())
            | if metadata.is_file() { 0x20 } else { 0 }
            | if self.name.as_encoded_bytes().first() == Some(&b'.') {
                2
            } else {
                0
            };
        [(1, 'r'), (0x20, 'a'), (4, 's'), (2, 'h')]
            .map(|(mask, letter)| if flags & mask == 0 { '.' } else { letter })
    }
}

pub(super) struct FilePicker {
    directory: PathBuf,
    entries: Vec<Entry>,
    order: Vec<usize>,
    selected: usize,
    first: usize,
    rows: usize,
    name_width: u16,
    size_width: u16,
    initial_selection: Option<OsString>,
    sort: Sort,
    reverse: bool,
    show_hidden: bool,
    limited: bool,
    filter: String,
    filter_input: Option<FilterInput>,
    error: Option<String>,
    help: Option<Help>,
}

impl FilePicker {
    fn new(directory: PathBuf, current_file: Option<&Path>) -> Self {
        let initial_selection = current_file
            .filter(|path| path.parent() == Some(directory.as_path()))
            .and_then(Path::file_name)
            .map(OsStr::to_owned);
        Self {
            directory,
            entries: Vec::new(),
            order: Vec::new(),
            selected: 0,
            first: 0,
            rows: 1,
            name_width: 12,
            size_width: 10,
            initial_selection,
            sort: Sort::Name,
            reverse: false,
            show_hidden: false,
            limited: false,
            filter: "*".into(),
            filter_input: None,
            error: None,
            help: None,
        }
    }

    fn selected_name(&self) -> Option<&OsStr> {
        self.order
            .get(self.selected)
            .map(|index| self.entries[*index].name.as_os_str())
    }

    fn reload(&mut self) {
        let selection = self
            .initial_selection
            .take()
            .or_else(|| self.selected_name().map(OsStr::to_owned));
        self.load_directory(self.directory.clone(), selection.as_deref());
    }

    fn load_directory(&mut self, directory: PathBuf, selection: Option<&OsStr>) {
        match read_directory(&directory) {
            Ok((entries, limited)) => {
                self.directory = directory;
                self.entries = entries;
                self.limited = limited;
                self.reorder(selection);
            }
            Err(error) => self.report_error(&directory, &error),
        }
    }

    fn report_error(&mut self, path: &Path, error: &io::Error) {
        self.error = Some(escaped_text(&format!("{}: {error}", path.display())));
    }

    fn reorder(&mut self, selection: Option<&OsStr>) {
        self.order = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                (self.show_hidden || entry.attributes()[3] != 'h')
                    && (entry.directory()
                        || self.filter == "*"
                        || filter::matches(&self.filter, &entry.name_key))
            })
            .map(|(index, _)| index)
            .collect();
        if self.sort != Sort::DirectoryOrder {
            let entries = &self.entries;
            self.order.sort_unstable_by(|left, right| {
                let a = &entries[*left];
                let b = &entries[*right];
                let name = || {
                    a.name_key
                        .cmp(&b.name_key)
                        .then_with(|| a.name.cmp(&b.name))
                };
                let order = match self.sort {
                    Sort::Extension => a.extension_key.cmp(&b.extension_key).then_with(name),
                    Sort::Time => b.modified.cmp(&a.modified).then_with(name),
                    Sort::Size => b.size().cmp(&a.size()).then_with(name),
                    Sort::Name | Sort::DirectoryOrder => name(),
                };
                a.metadata
                    .is_some()
                    .cmp(&b.metadata.is_some())
                    .then_with(|| b.directory().cmp(&a.directory()))
                    .then(if self.reverse { order.reverse() } else { order })
            });
        }
        self.name_width = 12;
        self.size_width = 10;
        for entry in &self.entries {
            self.name_width = self.name_width.max(entry.screen_width);
            if !entry.directory() {
                let digits = entry.size().checked_ilog10().unwrap_or(0) + 1;
                self.size_width = self.size_width.max(digits as u16);
            }
        }
        self.selected = selection
            .and_then(|name| {
                self.order
                    .iter()
                    .position(|index| self.entries[*index].name == name)
            })
            .unwrap_or(0);
        self.scroll();
    }

    fn scroll(&mut self) {
        self.first = self
            .first
            .clamp(self.selected.saturating_sub(self.rows - 1), self.selected);
        self.first = self.first.min(self.order.len().saturating_sub(self.rows));
    }

    fn key(&mut self, key: KeyEvent) -> Action {
        if key.kind == KeyEventKind::Release {
            return Action::Continue;
        }
        if let Some(help) = &mut self.help {
            if help.closes(key) {
                self.help = None;
            }
            return Action::Continue;
        }
        if let Some(mut input) = self.filter_input.take() {
            if key.code == KeyCode::F(1) && key.modifiers.is_empty() {
                self.help = Some(Help::new(HelpTopic::FileFilter));
                self.filter_input = Some(input);
            } else if key.code == KeyCode::Enter && key.modifiers.is_empty() {
                let selection = self.selected_name().map(OsStr::to_owned);
                self.filter = input.finish();
                self.reorder(selection.as_deref());
            } else if key.code != KeyCode::Esc && !is_control_char(key, 'c') {
                input.edit(key);
                self.filter_input = Some(input);
            }
            return Action::Continue;
        }
        if self.error.take().is_some() {
            return Action::Continue;
        }
        if is_control_char(key, 'c') {
            return Action::Close;
        }
        if (key.code, key.modifiers) == (KeyCode::F(4), KeyModifiers::ALT) {
            self.reload();
            return Action::Continue;
        }
        if !key.modifiers.is_empty() {
            return Action::Continue;
        }
        let last = self.order.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Enter => return self.open_selected(),
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(last),
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(self.rows),
            KeyCode::PageDown => self.selected = (self.selected + self.rows).min(last),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = last,
            KeyCode::F(1) => self.help = Some(Help::new(HelpTopic::Files)),
            KeyCode::F(2..=8) => self.change_order(key.code),
            KeyCode::F(10) => self.filter_input = Some(FilterInput::new(&self.filter)),
            _ => {}
        }
        self.scroll();
        Action::Continue
    }

    fn change_order(&mut self, key: KeyCode) {
        let selection = self.selected_name().map(OsStr::to_owned);
        match key {
            KeyCode::F(2) => self.show_hidden = !self.show_hidden,
            KeyCode::F(8) => self.reverse = !self.reverse,
            _ => {
                self.sort = match key {
                    KeyCode::F(3) => Sort::Name,
                    KeyCode::F(4) => Sort::Extension,
                    KeyCode::F(5) => Sort::Time,
                    KeyCode::F(6) => Sort::Size,
                    _ => Sort::DirectoryOrder,
                };
                self.reverse = false;
            }
        }
        self.reorder(selection.as_deref());
    }

    fn open_selected(&mut self) -> Action {
        let Some(entry) = self
            .order
            .get(self.selected)
            .map(|index| &self.entries[*index])
        else {
            return Action::Continue;
        };
        if entry.metadata.is_none() {
            if let Some(parent) = self.directory.parent() {
                let selection = self.directory.file_name().map(OsStr::to_owned);
                self.load_directory(parent.to_path_buf(), selection.as_deref());
            }
        } else if entry.directory() {
            self.load_directory(self.directory.join(&entry.name), None);
        } else {
            return Action::Open(self.directory.join(&entry.name));
        }
        Action::Continue
    }

    fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        if area.is_empty() {
            return;
        }
        let name_width = self.name_width.min(area.width.saturating_sub(44).max(12));
        let size_width = self.size_width;
        let centered = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(1),
        );
        let columns = dialog_surface(
            frame,
            centered,
            name_width.saturating_add(size_width + 34),
            area.height.saturating_sub(4).max(5),
            Line::raw(escaped_text(&self.directory.to_string_lossy())).centered(),
            HEADER,
        );
        let body = columns.inner(Margin::new(1, 0));
        self.rows = usize::from(body.height).max(1);
        self.scroll();

        if !body.is_empty() {
            let marker = if self.limited {
                "+"
            } else {
                self.filter.as_str()
            };
            let width = (marker.len() as u16).min(body.width);
            let x = body.x + (body.width - width) / 2;
            frame.render_widget(
                Paragraph::new(marker).style(HEADER),
                Rect::new(x, body.bottom(), width, 1),
            );
            for (row, index) in self
                .order
                .iter()
                .enumerate()
                .skip(self.first)
                .take(usize::from(body.height))
            {
                render_entry(
                    frame,
                    Rect::new(body.x, body.y + (row - self.first) as u16, body.width, 1),
                    &self.entries[*index],
                    name_width,
                    size_width,
                    row == self.selected,
                );
            }
        }
        let bar = Rect::new(area.x, area.bottom() - 1, area.width, 1);
        if let Some(input) = &self.filter_input {
            input.render(frame, area, self.help.is_none());
        }
        if let Some(help) = &mut self.help {
            render_full_function_bar(frame, bar, &[""; 12], &[]);
            help.render(frame, area);
        } else if self.filter_input.is_some() {
            let mut labels = [""; 12];
            labels[0] = "Help";
            render_full_function_bar(frame, bar, &labels, &[]);
        } else {
            render_full_function_bar(
                frame,
                bar,
                &[
                    "Help", "Hidden", "byName", "byExt", "byTime", "Size", "Unsort", "Revers",
                    "FilHis", "Filter", "", "",
                ],
                &[9],
            );
        }
        if let Some(error) = &self.error {
            let rect = dialog_surface(frame, area, 66, 7, Line::default(), ERROR);
            frame.render_widget(
                Paragraph::new(error.as_str())
                    .wrap(ratatui::widgets::Wrap { trim: true })
                    .style(ERROR),
                rect,
            );
        }
    }
}

fn read_directory(path: &Path) -> io::Result<(Vec<Entry>, bool)> {
    let mut entries = Vec::new();
    if path.parent().is_some() {
        entries.push(Entry::new("..".into(), None));
    }
    let mut files = fs::read_dir(path)?;
    for entry in files.by_ref().take(MAX_ENTRIES) {
        let entry = entry?;
        // A concurrent rename or dangling symbolic link need not hide its siblings.
        let Ok(metadata) = fs::metadata(entry.path()) else {
            continue;
        };
        if metadata.is_file() || metadata.is_dir() {
            entries.push(Entry::new(entry.file_name(), Some(metadata)));
        }
    }
    Ok((entries, files.next().is_some()))
}

fn render_entry(
    frame: &mut Frame<'_>,
    area: Rect,
    entry: &Entry,
    name_width: u16,
    size_width: u16,
    selected: bool,
) {
    let name_width = name_width.min(area.width);
    let style = if selected { FOCUSED_ROW } else { BODY };
    let (size, attrs, date, time) = match &entry.metadata {
        None => (
            ">UP---DIR<".to_string(),
            "Attr".into(),
            "---Date---".into(),
            "--Time--".into(),
        ),
        Some(metadata) => {
            let size = if metadata.is_dir() {
                ">SUB--DIR<".into()
            } else {
                entry.size().to_string()
            };
            let (date, time) = timestamp::columns(metadata);
            (
                size,
                entry.attributes().iter().collect::<String>(),
                date,
                time,
            )
        }
    };
    let rest = format!(
        "│{size:>width$}│{attrs}│{date}│{time}",
        width = usize::from(size_width)
    );
    frame.render_widget(Block::default().style(style), area);
    frame.render_widget(
        Paragraph::new(entry.display.as_str()).style(style),
        Rect::new(area.x, area.y, name_width, 1),
    );
    frame.render_widget(
        Paragraph::new(rest).style(style),
        Rect::new(
            area.x + name_width,
            area.y,
            area.width.saturating_sub(name_width),
            1,
        ),
    );
}

#[cfg(test)]
#[path = "file_picker/tests.rs"]
mod tests;
