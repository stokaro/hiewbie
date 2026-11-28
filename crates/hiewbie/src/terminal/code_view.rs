use hiewbie_core::formats::BinaryFormat;
use hiewbie_core::instructions::{DecodeMode, DecodeStatus};
use hiewbie_core::{BRANCH_LABELS, ByteSource, CodeRow, FileOffset};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::hex_view::{BYTE_BODY, address_digits, pc_glyph, render_scrollbar};
use super::render::{MARK, set_body_caret};
use super::{EntryColumn, Ui, escaped_text};

const SELECTED_BYTE: Style = Style::new().fg(Color::Blue).bg(Color::Gray);
const MODIFIED_BYTE: Style = BYTE_BODY.fg(Color::LightYellow);
const BRANCH: Style = BYTE_BODY.fg(Color::Yellow);

#[derive(Default)]
pub(super) struct CodeViewport {
    pub next_top: Option<FileOffset>,
    pub end: u64,
    pub selected_line: usize,
}

impl<S: ByteSource> Ui<S> {
    pub(super) fn render_code(&mut self, frame: &mut Frame<'_>, body: Rect) {
        let length = self.editor.document().len();
        if self.reopen_required || length == 0 {
            let text = if self.reopen_required {
                "<file was replaced; exit and reopen before analysis>"
            } else {
                "<empty file - no instructions>"
            };
            frame.render_widget(
                Paragraph::new(text).style(BYTE_BODY),
                Rect::new(body.x + 1, body.y, body.width - 1, 1),
            );
            render_scrollbar(frame, body, 0, length, 0);
            return;
        }
        let cursor = self.editor.cursor();
        if cursor.get() < self.top {
            self.top = cursor.get();
        }
        let (rows, end) = match self.decode_code_viewport(cursor) {
            Ok(result) => result,
            Err(error) => {
                self.notice(format!("Code unavailable; raw bytes: {error}"));
                self.render_bytes(frame, frame.area());
                return;
            }
        };
        if self.message.starts_with("Code unavailable; raw bytes:") {
            self.hint("Code analysis available");
        }
        let context = self.editor.decode_context();
        let editing = self.editor.is_editing();
        let wide = self
            .editor
            .executable()
            .is_some_and(|image| image.format == BinaryFormat::Pe64)
            || length.saturating_sub(1) > u64::from(u32::MAX)
            || rows
                .iter()
                .any(|row| row.instruction.address.get() > u64::from(u32::MAX));
        let op_column = if wide { 21 } else { 12 };
        let block = self
            .editor
            .block_selection()
            .map(|selection| selection.range);
        let mut lines = Vec::with_capacity(rows.len());
        let mut caret = None;
        for (row_index, row) in rows.iter().enumerate() {
            let offset = row.offset.get();
            let address = if editing {
                offset
            } else {
                row.instruction.address.get()
            };
            let mapped = row.mapped && !editing && (!context.raw || context.base.get() != 0);
            let marker = if mapped { '.' } else { ' ' };
            let mut spans = vec![Span::raw(format!(
                " {marker}{}: ",
                address_digits(address, wide)
            ))];
            for (byte_index, byte) in row.instruction.bytes().iter().copied().enumerate() {
                let position = FileOffset::new(offset + byte_index as u64);
                let current = cursor == position;
                let pending = self.pending_nibble.filter(|_| current);
                let shown = pending.map_or(byte, |digit| (digit << 4) | (byte & 15));
                let style = if current && !editing {
                    SELECTED_BYTE
                } else if block.is_some_and(|range| range.contains(position)) {
                    MARK
                } else if editing
                    && (shown != byte || self.editor.document().is_byte_modified(position))
                {
                    MODIFIED_BYTE
                } else {
                    BYTE_BODY
                };
                spans.push(Span::styled(format!("{shown:02X}"), style));
                if current && self.editing_caret_visible() {
                    let nibble =
                        usize::from(self.entry_column == EntryColumn::Hex && pending.is_some());
                    caret = Some((op_column + 2 * byte_index + nibble, row_index));
                }
            }
            let used = row.instruction.length * 2;
            spans.push(Span::raw(" ".repeat(30 - used)));
            let incoming = rows.iter().find_map(|source| {
                (source.instruction.direct_branch.as_ref()?.address == row.instruction.address)
                    .then(|| branch_label(source))
                    .flatten()
            });
            spans.push(incoming.map_or_else(
                || Span::raw(" "),
                |label| Span::styled(label.to_string(), BRANCH),
            ));
            let mut instruction =
                instruction_spans(row, context.mode, incoming.is_some(), mapped, wide);
            if row.instruction.bytes() == [0xC3] && context.mode.is_some_and(DecodeMode::is_x86) {
                instruction.extend([Span::raw(" ; "), Span::raw(format!("{}-", "-^".repeat(15)))]);
            }
            spans.extend(instruction);
            lines.push(Line::from(spans));
        }
        if let Some((x, y)) = caret {
            set_body_caret(frame, body, x, y);
        }
        self.code_viewport = CodeViewport {
            next_top: rows.get(1).map(|row| row.offset),
            end,
            selected_line: rows
                .iter()
                .position(|row| {
                    let start = row.offset.get();
                    (start..start + row.instruction.length as u64).contains(&cursor.get())
                })
                .unwrap_or(0),
        };
        frame.render_widget(Paragraph::new(lines).style(BYTE_BODY), body);
        render_scrollbar(frame, body, self.top, length, end.saturating_sub(self.top));
    }

    fn decode_code_viewport(
        &mut self,
        cursor: FileOffset,
    ) -> Result<(Vec<CodeRow>, u64), hiewbie_core::Error> {
        let mut rows = self
            .editor
            .code_viewport(FileOffset::new(self.top), self.visible_rows)?;
        let mut end = code_window_end(&rows);
        if self.top != cursor.get() && cursor.get() >= end {
            self.top = cursor.get();
            rows = self.editor.code_viewport(cursor, self.visible_rows)?;
            end = code_window_end(&rows);
        }
        Ok((rows, end))
    }
}

fn code_window_end(rows: &[CodeRow]) -> u64 {
    rows.last()
        .map_or(0, |row| row.offset.get() + row.instruction.length as u64)
}

fn branch_label(row: &CodeRow) -> Option<char> {
    let number = row.branch_number?;
    BRANCH_LABELS
        .get(number.checked_sub(1)?)
        .copied()
        .map(char::from)
}

fn instruction_spans(
    row: &CodeRow,
    mode: Option<DecodeMode>,
    marked: bool,
    mapped: bool,
    wide: bool,
) -> Vec<Span<'static>> {
    let instruction = &row.instruction;
    if instruction.status != DecodeStatus::Decoded {
        let state = match instruction.status {
            DecodeStatus::Decoded => unreachable!(),
            DecodeStatus::Unknown => "unknown: ",
            DecodeStatus::Truncated => "truncated: ",
            DecodeStatus::Unsupported => "unsupported: ",
        };
        return vec![Span::raw(format!(
            "{state}{}",
            escaped_text(&instruction.text)
        ))];
    }
    let (name, operands) = instruction
        .text
        .split_once(' ')
        .unwrap_or((&instruction.text, ""));
    if !mode.is_some_and(DecodeMode::is_x86) {
        let mut spans = vec![Span::raw(escaped_text(&instruction.text))];
        append_branch_indicator(&mut spans, row);
        return spans;
    }
    let (name, operands) = match name {
        "int3" => ("int", "3"),
        "ret" => ("retn", operands),
        "je" => ("jz", operands),
        "jne" => ("jnz", operands),
        "jmp" if instruction.bytes().first() == Some(&0xEB) => ("jmps", operands),
        _ => (name, operands),
    };
    let mnemonic_style = if marked || name == "retn" {
        BYTE_BODY.fg(Color::Cyan)
    } else {
        BYTE_BODY
    };
    let mut spans = vec![Span::styled(
        if operands.is_empty() {
            name.to_owned()
        } else if mapped && instruction.direct_branch.is_some() {
            format!("{name:<12}")
        } else {
            format!("{name:<13}")
        },
        mnemonic_style,
    )];
    if let Some(target) = instruction.direct_branch.as_ref() {
        let address = target.address.get();
        let address = if mapped {
            format!(".{}", address_digits(address, wide))
        } else {
            format!("0{}", address_digits(address, wide))
        };
        spans.push(Span::styled(address, BYTE_BODY.fg(Color::LightGreen)));
        append_branch_indicator(&mut spans, row);
    } else {
        spans.extend(operand_spans(operands));
    }
    spans
}

fn append_branch_indicator(spans: &mut Vec<Span<'static>>, row: &CodeRow) {
    if let Some(label) = branch_label(row)
        && let Some(target) = row.instruction.direct_branch.as_ref()
    {
        let arrow = if target.address < row.instruction.address {
            '↑'
        } else {
            '↓'
        };
        spans.push(Span::raw(format!(" --{arrow}")));
        spans.push(Span::styled(label.to_string(), BRANCH));
    }
}

fn operand_spans(operands: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut source = operands;
    while !source.is_empty() {
        if source.starts_with('[')
            && let Some(end) = source.find(']')
        {
            let memory = &source[1..end];
            let text: String = memory
                .split('+')
                .map(|term| {
                    if let Some(number) = term.strip_prefix("0x") {
                        format!("[0{}]", number.to_ascii_uppercase())
                    } else {
                        format!("[{}]", term.replace("0x", "0"))
                    }
                })
                .collect();
            spans.push(Span::styled(text, BYTE_BODY.fg(Color::LightMagenta)));
            source = &source[end + 1..];
        } else if let Some(remaining) = source.strip_prefix("0x") {
            let end = remaining
                .find(|value: char| !value.is_ascii_hexdigit())
                .unwrap_or(remaining.len());
            let number = &remaining[..end];
            spans.push(Span::styled(
                format!("0{}", number.to_ascii_uppercase()),
                BYTE_BODY.fg(Color::LightGreen),
            ));
            if end == remaining.len()
                && let Ok(value) = u64::from_str_radix(number, 16)
                && value >= 0x20
            {
                let bytes = value.to_be_bytes();
                let first = bytes.iter().position(|byte| *byte != 0).unwrap_or(0);
                let text: String = bytes[first..].iter().map(|byte| pc_glyph(*byte)).collect();
                spans.push(Span::raw(" ;"));
                spans.push(Span::styled(
                    format!("'{text}'"),
                    BYTE_BODY.fg(Color::White),
                ));
            }
            source = &remaining[end..];
        } else {
            let end = source
                .find(|value: char| !value.is_ascii_alphanumeric())
                .unwrap_or(source.len());
            let value = source.chars().next().unwrap();
            let length = if end == 0 { value.len_utf8() } else { end };
            let style = if value.is_ascii_digit() {
                BYTE_BODY.fg(Color::LightGreen)
            } else if value.is_ascii_alphabetic() {
                BYTE_BODY.fg(Color::Reset)
            } else {
                BYTE_BODY
            };
            spans.push(Span::styled(escaped_text(&source[..length]), style));
            source = &source[length..];
        }
    }
    spans
}
