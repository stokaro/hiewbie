use hiewbie_core::ByteSource;
use hiewbie_core::calculator::{CalculationError, MAX_EXPRESSION_BYTES};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use super::help::{Help, HelpTopic};
use super::hex_view::pc_glyph;
use super::render::{PANEL, dialog_surface};
use super::{Dialog, Ui, escaped_text, is_control_char};

const FIELD: Style = Style::new().fg(Color::White).bg(Color::Cyan);
const UNSIGNED_GUIDE: &str = "                            .       |       .       .       .";

pub(super) struct Calculator {
    expression: String,
    cursor: usize,
    insert: bool,
    answer: Result<u64, CalculationError>,
    pub(super) help: Option<Help>,
}

impl Default for Calculator {
    fn default() -> Self {
        Self {
            expression: String::new(),
            cursor: 0,
            insert: false,
            answer: Ok(0),
            help: None,
        }
    }
}

impl Calculator {
    pub(super) fn reopen(&mut self) {
        self.cursor = self.expression.len();
    }

    fn type_character(&mut self, character: char) {
        if !self.insert && self.cursor < self.expression.len() {
            self.expression
                .replace_range(self.cursor..self.cursor + 1, &character.to_string());
        } else if self.expression.len() < MAX_EXPRESSION_BYTES {
            self.expression.insert(self.cursor, character);
        } else {
            return;
        }
        self.cursor += 1;
    }

    fn clear(&mut self) {
        self.expression.clear();
        self.cursor = 0;
    }
}

impl<S: ByteSource> Ui<S> {
    pub(super) fn calculator_key(&mut self, key: KeyEvent) {
        let calculator = &mut self.calculator;
        if let Some(help) = &mut calculator.help {
            if help.closes(key) {
                calculator.help = None;
            }
        } else if key.code == KeyCode::Esc || is_control_char(key, 'c') {
            return;
        } else if is_control_char(key, 'u') {
            calculator.clear();
        } else {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, KeyModifiers::NONE) => {
                    calculator.answer = self.editor.calculate(&calculator.expression);
                }
                (KeyCode::F(1), KeyModifiers::NONE) => {
                    calculator.help = Some(Help::new(HelpTopic::Calculator));
                }
                (KeyCode::Insert, KeyModifiers::NONE) => calculator.insert = !calculator.insert,
                (KeyCode::Home, KeyModifiers::NONE) => calculator.cursor = 0,
                (KeyCode::End, KeyModifiers::NONE) => {
                    calculator.cursor = calculator.expression.len()
                }
                (KeyCode::Left, KeyModifiers::NONE) => {
                    calculator.cursor = calculator.cursor.saturating_sub(1);
                }
                (KeyCode::Right, KeyModifiers::NONE) => {
                    calculator.cursor = (calculator.cursor + 1).min(calculator.expression.len());
                }
                (KeyCode::Backspace, KeyModifiers::NONE) if calculator.cursor > 0 => {
                    calculator.cursor -= 1;
                    calculator.expression.remove(calculator.cursor);
                }
                (KeyCode::Delete, KeyModifiers::NONE)
                    if calculator.cursor < calculator.expression.len() =>
                {
                    calculator.expression.remove(calculator.cursor);
                }
                (KeyCode::PageUp, KeyModifiers::NONE) => calculator.clear(),
                (KeyCode::End, KeyModifiers::CONTROL) => {
                    calculator.expression.truncate(calculator.cursor);
                }
                (KeyCode::Char(character), KeyModifiers::NONE | KeyModifiers::SHIFT)
                    if character == ' ' || character.is_ascii_graphic() =>
                {
                    calculator.type_character(character);
                }
                _ => {}
            }
        }
        self.dialog = Some(Dialog::Calculator);
    }
}

pub(super) fn render_calculator(frame: &mut Frame<'_>, area: Rect, calculator: &Calculator) {
    // HIEW uses the lower middle row when the console has an even height.
    let centered = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    );
    let content = dialog_surface(
        frame,
        centered,
        76,
        9,
        Line::from(" Calculator ").centered(),
        PANEL,
    )
    .inner(Margin::new(1, 0));
    if content.is_empty() {
        return;
    }

    let width = usize::from(content.width);
    let first = calculator.cursor.saturating_sub(width - 1);
    let last = (first + width).min(calculator.expression.len());
    let mut field = calculator.expression[first..last].to_owned();
    field.push_str(&"░".repeat(width - (last - first)));
    frame.render_widget(
        Paragraph::new(field).style(FIELD),
        Rect::new(content.x, content.y, content.width, 1),
    );
    if calculator.help.is_none() {
        frame.set_cursor_position((content.x + (calculator.cursor - first) as u16, content.y));
    }

    let lines = match &calculator.answer {
        Ok(value) => result_lines(*value),
        Err(error) => error_lines(error, first, width),
    };
    frame.render_widget(
        Paragraph::new(lines).style(PANEL),
        Rect::new(content.x, content.y + 1, content.width, content.height - 1),
    );
}

fn result_lines(value: u64) -> Vec<Line<'static>> {
    let bytes: String = value.to_be_bytes().into_iter().map(pc_glyph).collect();
    let unsigned = format!("Uns:{value}");
    vec![
        Line::raw(format!("Sig:{}", value as i64)),
        Line::raw(format!("{unsigned}{}", &UNSIGNED_GUIDE[unsigned.len()..])),
        Line::raw(format!("Bin:{value:064b}")),
        Line::raw(format!(
            "Hex:{:08X}`{:08X}     \"{bytes}\"",
            value >> 32,
            value as u32
        )),
    ]
}

fn error_lines(error: &CalculationError, first: usize, width: usize) -> Vec<Line<'static>> {
    let marker = error
        .expression_offset()
        .and_then(|offset| offset.checked_sub(first))
        .filter(|column| *column < width)
        .map_or_else(String::new, |column| format!("{}^", " ".repeat(column)));
    let description = match error {
        CalculationError::Syntax { .. } => "Syntax error".to_owned(),
        CalculationError::InvalidOperator { .. } => "Invalid operator".to_owned(),
        CalculationError::InvalidTag { .. } => "Invalid tag".to_owned(),
        CalculationError::MissingBracket { .. } => "No closing bracket".to_owned(),
        CalculationError::BigNumber { .. } => "Big number".to_owned(),
        CalculationError::ShortRead { .. } => "No value under cursor".to_owned(),
        _ => escaped_text(&error.to_string()),
    };
    vec![
        Line::raw(marker),
        Line::raw(format!("Error: {description}")),
    ]
}
