use std::collections::VecDeque;
use std::ops::Range;

use hiewbie_core::{ByteSource, Command, Error, FileOffset, MAX_READ_BYTES};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Margin, Rect};
use ratatui::style::Color;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use super::hex_view::{BYTE_BODY, draw_scrollbar, pc_glyph};
use super::render::{FOCUSED_ROW, HEADER, MARK, dialog_surface};
use super::{Dialog, Ui, UiAction};

const LINE_COLUMNS: usize = 512;
const ROW_BYTES: usize = LINE_COLUMNS + 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LineFeed {
    CrLf,
    Cr,
    Lf,
    Null,
}

impl LineFeed {
    const ALL: [Self; 4] = [Self::CrLf, Self::Cr, Self::Lf, Self::Null];
    pub(super) const LABELS: [&str; 4] = ["CR/LF", "CR", "LF", "NULL"];

    fn len_at(self, bytes: &[u8]) -> usize {
        match (self, bytes) {
            (Self::CrLf, [b'\r', b'\n', ..]) => 2,
            (Self::Cr, [b'\r', ..]) | (Self::Lf, [b'\n', ..]) | (Self::Null, [0, ..]) => 1,
            _ => 0,
        }
    }

    pub(super) fn index(self) -> usize {
        Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy)]
struct TextOptions {
    line_feed: LineFeed,
    wrap: bool,
    tabs: bool,
}

impl TextOptions {
    fn detect(bytes: &[u8]) -> Self {
        let text = bytes
            .iter()
            .all(|byte| matches!(byte, b'\t' | b'\r' | b'\n' | 0x20..=0x7e));
        let line_feed = if text {
            match bytes.iter().position(|byte| matches!(byte, b'\r' | b'\n')) {
                Some(index) if bytes[index..].starts_with(b"\r\n") => LineFeed::CrLf,
                Some(index) if bytes[index] == b'\n' => LineFeed::Lf,
                Some(_) => LineFeed::Cr,
                None => LineFeed::CrLf,
            }
        } else {
            LineFeed::CrLf
        };
        Self {
            line_feed,
            wrap: !text,
            tabs: text,
        }
    }
}

pub(super) struct TextView {
    options: Option<TextOptions>,
    columns: usize,
    horizontal: usize,
}

impl Default for TextView {
    fn default() -> Self {
        Self {
            options: None,
            columns: 79,
            horizontal: 0,
        }
    }
}

impl TextView {
    fn layout(&self, options: TextOptions) -> TextLayout {
        TextLayout {
            options,
            columns: if options.wrap {
                self.columns
            } else {
                LINE_COLUMNS
            },
        }
    }

    pub(super) fn horizontal(&self) -> Option<usize> {
        self.options
            .filter(|options| !options.wrap)
            .map(|_| self.horizontal)
    }
}

struct TextLayout {
    options: TextOptions,
    columns: usize,
}

impl TextLayout {
    fn consume(&self, bytes: &[u8], mut emit: impl FnMut(usize, Range<usize>, char)) -> usize {
        let mut column = 0;
        for (index, byte) in bytes.iter().copied().enumerate() {
            let ending = self.options.line_feed.len_at(&bytes[index..]);
            if ending != 0 {
                return index + ending;
            }
            if column == self.columns {
                return index;
            }
            let (width, glyph) = if byte == b'\t' && self.options.tabs {
                (8 - column % 8, ' ')
            } else {
                (1, pc_glyph(byte))
            };
            let next = (column + width).min(self.columns);
            emit(index, column..next, glyph);
            column = next;
        }
        bytes.len()
    }

    fn next(&self, bytes: &[u8]) -> usize {
        self.consume(bytes, |_, _, _| {})
    }

    fn previous(&self, bytes: &[u8], at: usize, count: usize, start_of_file: bool) -> usize {
        // If the read starts in an unfinished physical line, skip to the first
        // known boundary. With no boundary in range, align from the current row.
        let first = if start_of_file {
            0
        } else {
            (0..at)
                .find_map(|index| {
                    let ending = self.options.line_feed.len_at(&bytes[index..at]);
                    (ending != 0 && index + ending < at).then_some(index + ending)
                })
                .unwrap_or(at.saturating_sub(count * self.columns))
        };
        let mut starts = VecDeque::with_capacity(count);
        let mut row = first;
        while row < at {
            if starts.len() == count {
                starts.pop_front();
            }
            starts.push_back(row);
            row += self.next(&bytes[row..]);
        }
        starts.front().copied().unwrap_or(0)
    }
}

impl<S: ByteSource> Ui<S> {
    pub(super) fn text_navigation(&self) -> bool {
        self.editor.mode() == hiewbie_core::ViewMode::Text && !self.editor.is_editing()
    }

    pub(super) fn prepare_text(&mut self, area: Rect) {
        self.text.columns = usize::from(area.width.saturating_sub(1)).clamp(1, LINE_COLUMNS);
        self.visible_rows =
            usize::from(area.height.saturating_sub(2)).clamp(1, MAX_READ_BYTES / ROW_BYTES);
        self.text_options();
    }

    fn text_options(&mut self) -> TextOptions {
        *self.text.options.get_or_insert_with(|| {
            let mut sample = [0; 4096];
            let count = self
                .editor
                .document_mut()
                .read_at(FileOffset::new(0), &mut sample);
            count.map_or_else(
                |_| TextOptions::detect(&[]),
                |count| TextOptions::detect(&sample[..count]),
            )
        })
    }

    pub(super) fn text_wrap_label(&self) -> &'static str {
        if self.text.horizontal().is_some() {
            "Wrap"
        } else {
            "Unwrap"
        }
    }

    pub(super) fn toggle_text_tabs(&mut self) -> UiAction {
        let mut options = self.text_options();
        options.tabs = !options.tabs;
        self.text.options = Some(options);
        UiAction::Continue
    }

    pub(super) fn text_key(&mut self, code: KeyCode) -> Option<UiAction> {
        let mut options = self.text_options();
        let column = &mut self.text.horizontal;
        match code {
            KeyCode::F(2) => {
                options.wrap = !options.wrap;
                *column = 0;
                self.text.options = Some(options);
            }
            KeyCode::F(3) => {}
            KeyCode::F(6) => self.dialog = Some(Dialog::LineFeed(options.line_feed)),
            KeyCode::F(8) => self.notice("Translation tables are not implemented"),
            KeyCode::Left if !options.wrap => *column = column.saturating_sub(1),
            KeyCode::Right if !options.wrap => *column = (*column + 1).min(LINE_COLUMNS - 1),
            KeyCode::Home if !options.wrap => *column = 0,
            KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End => {}
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                let count = match code {
                    KeyCode::PageUp | KeyCode::PageDown => self.visible_rows,
                    _ => 1,
                };
                return Some(self.move_text(code, count, options));
            }
            _ => return None,
        }
        Some(UiAction::Continue)
    }

    pub(super) fn text_line_feed_key(&mut self, mut selected: LineFeed, key: KeyEvent) {
        let index = selected.index();
        if key.modifiers.is_empty() {
            let choice = match key.code {
                KeyCode::F(number @ 1..=4) => Some(usize::from(number - 1)),
                KeyCode::Enter => Some(index),
                KeyCode::F(6) => Some((index + 1) % LineFeed::ALL.len()),
                KeyCode::Up => {
                    selected = LineFeed::ALL[index.saturating_sub(1)];
                    None
                }
                KeyCode::Down => {
                    selected = LineFeed::ALL[(index + 1).min(LineFeed::ALL.len() - 1)];
                    None
                }
                _ => None,
            };
            if let Some(index) = choice {
                let mut options = self.text_options();
                options.line_feed = LineFeed::ALL[index];
                self.text.options = Some(options);
                return;
            }
        }
        self.dialog = Some(Dialog::LineFeed(selected));
    }

    fn read_text(&mut self, offset: u64, limit: usize) -> Result<Vec<u8>, Error> {
        let available = self.editor.document().len().saturating_sub(offset);
        let mut data = vec![0; available.min(limit as u64) as usize];
        let count = self
            .editor
            .document_mut()
            .read_at(FileOffset::new(offset), &mut data)?;
        data.truncate(count);
        Ok(data)
    }

    fn move_text(&mut self, code: KeyCode, count: usize, options: TextOptions) -> UiAction {
        let layout = self.text.layout(options);
        let cursor = self.editor.cursor().get();
        let backward = matches!(code, KeyCode::Up | KeyCode::PageUp);
        let start = if backward {
            cursor.saturating_sub(((count + 1) * ROW_BYTES).min(MAX_READ_BYTES - 2) as u64)
        } else {
            cursor
        };
        let limit = if backward {
            (cursor - start) as usize + 2
        } else {
            (count * ROW_BYTES + 1).min(MAX_READ_BYTES)
        };
        let data = match self.read_text(start, limit) {
            Ok(data) => data,
            Err(error) => {
                self.notice(format!("Read failed: {error}"));
                return UiAction::Continue;
            }
        };
        let next = if backward {
            let at = (cursor - start) as usize;
            if at > data.len() {
                return UiAction::Continue;
            }
            layout.previous(&data, at, count, start == 0)
        } else {
            let mut offset = 0;
            let length = self.editor.document().len();
            for _ in 0..count {
                let next = offset + layout.next(&data[offset..]);
                if next == offset || start.saturating_add(next as u64) >= length {
                    break;
                }
                offset = next;
            }
            offset
        };
        self.dispatch(Command::Goto(FileOffset::new(
            start.saturating_add(next as u64),
        )))
    }

    pub(super) fn render_text(&mut self, frame: &mut Frame<'_>, body: Rect) {
        let cursor = self.editor.cursor().get();
        let length = self.editor.document().len();
        if self.reopen_required {
            frame.render_widget(
                Paragraph::new("<file was replaced; exit and reopen to verify its saved state>")
                    .style(BYTE_BODY),
                body,
            );
            return;
        }
        let options = self.text_options();
        let layout = self.text.layout(options);
        let data = match self.read_text(cursor, self.visible_rows * ROW_BYTES) {
            Ok(data) => data,
            Err(error) => {
                self.notice(format!("Read failed: {error}"));
                draw_scrollbar(frame, body, 0..0);
                return;
            }
        };
        let scroll = self.text.horizontal().unwrap_or(0);
        let right = scroll + usize::from(body.width.saturating_sub(1));
        let marked = self.editor.block_selection().map(|block| block.range);
        let mut offset = 0;
        for row in 0..self.visible_rows {
            if offset == data.len() {
                break;
            }
            let y = body.y + row as u16;
            offset += layout.consume(&data[offset..], |index, columns, glyph| {
                let position = FileOffset::new(cursor + (offset + index) as u64);
                let style = if marked.is_some_and(|range| range.contains(position)) {
                    MARK
                } else {
                    BYTE_BODY
                };
                for column in columns.start.max(scroll)..columns.end.min(right) {
                    let x = body.x + 1 + (column - scroll) as u16;
                    frame.buffer_mut()[(x, y)].set_char(glyph).set_style(style);
                }
            });
        }
        let track = u128::from(body.height.saturating_sub(3));
        let end = u128::from(cursor.saturating_add(offset as u64).min(length));
        let filled = (end * track / u128::from(length.max(1))) as u16;
        draw_scrollbar(frame, body, 0..filled);
    }
}

pub(super) fn render_line_feed(frame: &mut Frame<'_>, area: Rect, selected: LineFeed) {
    let content = dialog_surface(
        frame,
        Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(1),
        ),
        27,
        8,
        Line::from(" Select \"LineFeed\" ").centered(),
        HEADER,
    )
    .inner(Margin::new(1, 0));
    for (index, label) in LineFeed::LABELS
        .iter()
        .take(usize::from(content.height))
        .enumerate()
    {
        let style = if index == selected.index() {
            FOCUSED_ROW
        } else {
            HEADER.fg(Color::Blue)
        };
        let padding = usize::from(content.width.saturating_sub(5) / 2);
        frame.render_widget(
            Paragraph::new(format!("{:padding$}{label}", "")).style(style),
            Rect::new(content.x, content.y + index as u16, content.width, 1),
        );
    }
}
