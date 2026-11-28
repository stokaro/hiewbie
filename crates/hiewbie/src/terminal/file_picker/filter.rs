use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::terminal::render::{PANEL, dialog_surface};

const MAX_PATTERN: usize = 40;
const FIELD: Style = Style::new().fg(Color::White).bg(Color::Cyan);

pub(super) struct FilterInput {
    pattern: String,
    cursor: usize,
    insert: bool,
}

impl FilterInput {
    pub(super) fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_owned(),
            cursor: pattern.len(),
            insert: false,
        }
    }

    pub(super) fn finish(self) -> String {
        if self.pattern.is_empty() {
            "*".into()
        } else {
            self.pattern
        }
    }

    pub(super) fn edit(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::End, KeyModifiers::CONTROL) | (KeyCode::Char('x'), KeyModifiers::ALT) => {
                self.pattern.truncate(self.cursor)
            }
            (KeyCode::Home, KeyModifiers::NONE) => self.cursor = 0,
            (KeyCode::End, KeyModifiers::NONE) => self.cursor = self.pattern.len(),
            (KeyCode::Left, KeyModifiers::NONE) => self.cursor = self.cursor.saturating_sub(1),
            (KeyCode::Right, KeyModifiers::NONE) => {
                self.cursor = (self.cursor + 1).min(self.pattern.len());
            }
            (KeyCode::Insert, KeyModifiers::NONE) => self.insert = !self.insert,
            (KeyCode::Backspace, KeyModifiers::NONE) if self.cursor > 0 => {
                self.cursor -= 1;
                self.pattern.remove(self.cursor);
            }
            (KeyCode::Delete, KeyModifiers::NONE) if self.cursor < self.pattern.len() => {
                self.pattern.remove(self.cursor);
            }
            (KeyCode::PageUp, KeyModifiers::NONE) => {
                self.pattern.clear();
                self.cursor = 0;
            }
            (KeyCode::Char(character), KeyModifiers::NONE | KeyModifiers::SHIFT)
                if character.is_ascii_graphic() || character == ' ' =>
            {
                if !self.insert && self.cursor < self.pattern.len() {
                    self.pattern.remove(self.cursor);
                } else if self.pattern.len() == MAX_PATTERN {
                    return;
                }
                self.pattern.insert(self.cursor, character);
                self.cursor += 1;
            }
            _ => {}
        }
    }

    pub(super) fn render(&self, frame: &mut Frame<'_>, area: Rect, show_caret: bool) {
        let center = Rect::new(
            area.x,
            area.y.saturating_add(1),
            area.width,
            area.height.saturating_sub(1),
        );
        let body = dialog_surface(
            frame,
            center,
            48,
            5,
            Line::raw("Enter filter").centered(),
            PANEL,
        )
        .inner(Margin::new(1, 0));
        if body.is_empty() {
            return;
        }
        let width = usize::from(body.width);
        let first = self.cursor.saturating_sub(width - 1);
        let last = self.pattern.len().min(first + width);
        let input = &self.pattern[first..last];
        let shaded = "░".repeat(width - input.len());
        frame.render_widget(
            Paragraph::new(format!("{input}{shaded}")).style(FIELD),
            body,
        );
        if show_caret {
            frame.set_cursor_position((body.x + (self.cursor - first) as u16, body.y));
        }
    }
}

pub(super) fn matches(pattern: &str, name: &str) -> bool {
    let pattern = pattern.as_bytes();
    let mut index = 0;
    let mut star = None;
    let mut remaining = name.chars();
    while let Some(character) = remaining.next() {
        while pattern.get(index) == Some(&b'*') {
            index += 1;
            // Retry this character from immediately after the last star if needed.
            star = Some((index, remaining.clone()));
        }
        match pattern.get(index) {
            Some(b'?') => index += 1,
            Some(byte) if char::from(*byte).eq_ignore_ascii_case(&character) => index += 1,
            _ => match &mut star {
                Some((retry_index, retry)) => {
                    index = *retry_index;
                    remaining = retry.clone();
                    retry.next();
                }
                None => return false,
            },
        }
    }
    pattern[index..].iter().all(|&byte| byte == b'*')
}
