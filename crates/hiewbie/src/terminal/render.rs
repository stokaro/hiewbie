use hiewbie_core::formats::FileRange;
use hiewbie_core::instructions::DecodeStatus;
use hiewbie_core::{
    ByteSource, FileOffset, MAX_CODE_ROWS, MAX_READ_BYTES, SearchDirection, ViewMode,
};
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::block::{BlockField, BlockWrite};
use super::calculator::render_calculator;
use super::hex_view::BYTE_BODY;
use super::search::{SearchDialog, SearchField};
use super::{
    Dialog, EntryColumn, InputDialog, ModeChoice, PromptKind, Ui, escaped_text, printable,
};

pub(super) const BASE: Style = Style::new().fg(Color::Gray).bg(Color::Blue);
const ADDRESS: Style = Style::new().fg(Color::Cyan).bg(Color::Blue);
const SELECTED: Style = Style::new().fg(Color::Black).bg(Color::Cyan);
pub(super) const MARK: Style = Style::new().fg(Color::Yellow).bg(Color::Magenta);
const ADDRESS_COLUMNS: usize = 18;
pub(super) const PANEL: Style = Style::new().fg(Color::Black).bg(Color::Gray);
pub(super) const FOCUSED_ROW: Style = Style::new().fg(Color::White).bg(Color::Black);
const ERROR_PANEL: Style = Style::new().fg(Color::LightYellow).bg(Color::Red);
const ACTIVE_SEARCH_FIELD: Style = Style::new().fg(Color::White).bg(Color::Cyan);
const ACTIVE: Style = Style::new().fg(Color::Black).bg(Color::Yellow);
pub(super) const HEADER: Style = Style::new().fg(Color::Black).bg(Color::Cyan);

impl<S: ByteSource> Ui<S> {
    pub(super) fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        if area.width == 0 || area.height == 0 {
            return;
        }
        let hex = self.editor.mode() == ViewMode::Hex;
        let text = self.text_navigation();
        if self.editor.mode() == ViewMode::Code
            || (hex && !self.editor.is_editing())
            || matches!(self.dialog, Some(Dialog::Analysis(_)))
        {
            self.editor.refresh_analysis();
        }
        if text {
            self.prepare_text(area);
        } else {
            self.update_viewport(area);
        }
        frame.render_widget(
            Block::default().style(if hex || text { BYTE_BODY } else { BASE }),
            area,
        );
        let header = format!(" hiewbie {} | {}", env!("CARGO_PKG_VERSION"), self.label);
        let title = row(area, 0);
        if hex || text {
            let scan = self
                .editor
                .is_searching()
                .then(|| self.scan_position.unwrap_or(self.editor.cursor()).get());
            if hex {
                self.render_hex_title(frame, title, scan);
            } else {
                self.render_text_title(frame, title, scan);
            }
        }
        if self.editor.is_searching() {
            let width = (area.width / 3).min(18);
            let next = self.scan_position.unwrap_or(self.editor.cursor()).get();
            let address = format!("{next:08X}│");
            let address_width = (address.chars().count() as u16).min(area.width - width);
            let title_width = area.width - width - address_width;
            if !hex && !text {
                frame.render_widget(
                    Paragraph::new(header).style(HEADER),
                    Rect::new(area.x, area.y, title_width, 1),
                );
                frame.render_widget(
                    Paragraph::new(address).right_aligned().style(PANEL),
                    Rect::new(area.x + title_width, area.y, address_width, 1),
                );
            }
            let len = self.editor.document().len().max(1);
            let filled = (u128::from(next.min(len)) * u128::from(width) / u128::from(len)) as usize;
            let bar = format!(
                "{}{}",
                "▓".repeat(filled),
                "░".repeat(usize::from(width) - filled)
            );
            frame.render_widget(
                Paragraph::new(bar).style(PANEL),
                Rect::new(area.right() - width, area.y, width, 1),
            );
        } else if self.editor.mode() == ViewMode::Code {
            self.render_code_title(frame, title);
        } else if !hex && !text {
            frame.render_widget(Paragraph::new(header).style(HEADER), title);
        }

        if area.width < 23 || area.height < 5 {
            if area.height > 1 {
                frame.render_widget(
                    Paragraph::new("Resize: at least 23 x 5").style(BASE),
                    row(area, 1),
                );
            }
            return;
        }

        if hex {
            self.render_hex(
                frame,
                Rect::new(area.x, area.y + 1, area.width, area.height - 2),
                &self.hex_layout(area.width),
            );
        } else if text {
            self.render_text(
                frame,
                Rect::new(area.x, area.y + 1, area.width, area.height - 2),
            );
        } else {
            self.render_status_details(frame, row(area, 1));
            if self.editor.mode() == ViewMode::Code {
                self.render_code(frame, area);
            } else {
                self.render_bytes(frame, area);
            }
        }
        if (!hex && !text)
            || self.show_hex_message
            || self
                .editor
                .block_selection()
                .is_some_and(|block| block.is_marking)
        {
            let last = area.height - 2;
            let cursor = self.editor.cursor().get().saturating_sub(self.top);
            let editing_last_row = hex
                && self.editing_caret_visible()
                && cursor / self.bytes_per_row as u64 == u64::from(last - 1);
            let mut message = row(area, last - u16::from(editing_last_row));
            if hex || text {
                message.x += 1;
                message.width -= 1;
            }
            self.render_status_message(frame, message);
        }
        if matches!(self.dialog, Some(Dialog::Calculator)) {
            render_calculator(frame, area, &self.calculator);
        }
        let help = match self.dialog.as_mut() {
            Some(Dialog::Help(help)) => Some(help),
            Some(Dialog::Search(search)) => search.help.as_mut(),
            Some(Dialog::Calculator) => self.calculator.help.as_mut(),
            _ => None,
        };
        if let Some(help) = help {
            render_full_function_bar(frame, row(area, area.height - 1), &[""; 12], &[]);
            help.render(frame, area);
            return;
        }
        self.render_function_bar(frame, row(area, area.height - 1));
        if self.editor.is_searching() {
            let content = draw_dialog(
                frame,
                Rect::new(area.x, area.y, area.width, area.height.saturating_sub(6)),
                29,
                5,
                Line::default(),
                PANEL,
                DialogFrame::WithoutClose,
            );
            if content.height != 0 {
                frame.render_widget(
                    Paragraph::new("Processing...").centered().style(PANEL),
                    content,
                );
            }
        }
        match self.dialog.as_ref() {
            Some(Dialog::Input(input)) => render_prompt(frame, area, input),
            Some(Dialog::Search(search)) => render_search(frame, area, search),
            Some(Dialog::SearchNotice(message)) => render_search_notice(frame, area, message),
            Some(Dialog::BlockWrite(dialog)) => {
                if !self.export_in_progress {
                    render_block_write(frame, area, dialog);
                }
            }
            Some(Dialog::Mode(selected)) => render_mode_picker(frame, area, *selected),
            Some(Dialog::LineFeed(selected)) => {
                super::text_view::render_line_feed(frame, area, *selected)
            }
            Some(Dialog::Analysis(dialog)) => self.render_analysis_dialog(frame, area, dialog),
            Some(Dialog::Calculator | Dialog::Help(_)) | None => {}
        }
    }

    fn render_status_details(&self, frame: &mut Frame<'_>, area: Rect) {
        let mode = match self.editor.mode() {
            ViewMode::Hex => "HEX",
            ViewMode::Text => "TEXT",
            ViewMode::Code => "CODE",
        };
        let state = if self.reopen_required {
            "REOPEN"
        } else if self.editor.is_editing() {
            "EDIT"
        } else {
            "VIEW"
        };
        let capability = if self.editor.is_read_only() {
            "RO"
        } else {
            "RW"
        };
        let dirty = if self.editor.document().is_dirty() {
            "DIRTY"
        } else {
            "CLEAN"
        };
        let entry = if self.editor.is_editing() {
            match self.entry_column {
                EntryColumn::Hex => " HEX ENTRY",
                EntryColumn::Ascii => " ASCII ENTRY",
            }
        } else {
            ""
        };
        let status = if self.editor.mode() == ViewMode::Code {
            let context = self.editor.decode_context();
            let architecture = context
                .mode
                .map_or_else(|| "SELECT ISA".to_owned(), |mode| mode.to_string());
            let provenance = if context.raw {
                format!(
                    "RAW@{:X}{}",
                    context.base.get(),
                    if context.overridden { "!" } else { "" }
                )
            } else if context.overridden {
                "OVERRIDE".to_owned()
            } else {
                "AUTO".to_owned()
            };
            format!(
                "FO {:016X} {mode} {state} {capability} {dirty} {architecture} {} {provenance}{entry}",
                self.editor.cursor().get(),
                context.endian
            )
        } else {
            format!(
                "{:016X} / {:016X}  {mode} {state} {capability} {dirty}{entry}",
                self.editor.cursor().get(),
                self.editor.document().len()
            )
        };
        frame.render_widget(Paragraph::new(status).style(ADDRESS), area);
    }

    fn render_status_message(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut message = area;
        if let Some(block) = self
            .editor
            .block_selection()
            .filter(|block| block.is_marking)
        {
            let counter = format!("{:08X}/{}", block.range.len, block.range.len);
            let width = (counter.len() as u16).min(area.width);
            message.width = area.width - width;
            frame.render_widget(
                Paragraph::new(counter).style(MARK),
                Rect::new(area.x + message.width, area.y, width, 1),
            );
        }
        frame.render_widget(
            Paragraph::new(self.message.as_str()).style(BASE.fg(Color::Yellow)),
            message,
        );
    }

    fn update_viewport(&mut self, area: Rect) {
        if self.editor.mode() == ViewMode::Code {
            self.bytes_per_row = 1;
            self.visible_rows = usize::from(area.height.saturating_sub(4)).clamp(1, MAX_CODE_ROWS);
            self.top = self.editor.cursor().get();
            return;
        }
        self.bytes_per_row = match self.editor.mode() {
            ViewMode::Text => (area.width as usize).saturating_sub(18).clamp(1, 256),
            ViewMode::Hex => self.hex_layout(area.width).bytes_per_row,
            ViewMode::Code => 1,
        };
        let frame_rows = if self.editor.mode() == ViewMode::Hex {
            2
        } else {
            4
        };
        self.visible_rows = (area.height.saturating_sub(frame_rows) as usize)
            .min(MAX_READ_BYTES / self.bytes_per_row)
            .max(1);
        let per_row = self.bytes_per_row as u64;
        self.top = self.top / per_row * per_row;
        let cursor = self.editor.cursor().get();
        if cursor < self.top {
            self.top = cursor / per_row * per_row;
        } else if cursor.saturating_sub(self.top) >= (self.visible_rows * self.bytes_per_row) as u64
        {
            let cursor_row = cursor / per_row;
            self.top = cursor_row.saturating_sub(self.visible_rows as u64 - 1) * per_row;
        }
    }

    fn render_code(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let body = Rect::new(area.x, area.y + 2, area.width, area.height - 4);
        if self.reopen_required {
            frame.render_widget(
                Paragraph::new("<file was replaced; exit and reopen before analysis>").style(BASE),
                body,
            );
            return;
        }
        if self.editor.document().is_empty() {
            frame.render_widget(
                Paragraph::new("<empty file - no instructions>").style(BASE),
                body,
            );
            return;
        }
        let rows = match self.editor.code_rows(self.visible_rows) {
            Ok(rows) => rows,
            Err(error) => {
                self.notice(format!("Code unavailable; raw bytes: {error}"));
                self.render_bytes(frame, area);
                return;
            }
        };
        // Clear only this renderer's transient failure notice after recovery;
        // keep unrelated save, search, or navigation notices intact.
        if self.message.starts_with("Code unavailable; raw bytes:") {
            self.notice("Code analysis available; byte editing does not assemble instructions");
        }
        let raw = self.editor.decode_context().raw;
        let mut lines = Vec::with_capacity(rows.len());
        let block = self
            .editor
            .block_selection()
            .map(|selection| selection.range);
        for (index, row) in rows.iter().enumerate() {
            let domain = if raw {
                "R:"
            } else if row.mapped {
                "V:"
            } else {
                "F:"
            };
            let branch = row
                .branch_number
                .map_or_else(|| "  ".to_owned(), |number| format!("{number}>"));
            let state = match row.instruction.status {
                DecodeStatus::Decoded => "",
                DecodeStatus::Unknown => "unknown: ",
                DecodeStatus::Truncated => "truncated: ",
                DecodeStatus::Unsupported => "unsupported: ",
            };
            let selected = if index == 0 { SELECTED } else { ADDRESS };
            let byte_style = if index == 0 && self.editor.is_editing() {
                ACTIVE
            } else {
                BASE
            };
            let location = format!(
                "{:08X} {domain}{:016X} ",
                row.offset.get(),
                row.instruction.address.get()
            );
            let branch = format!("{branch} ");
            let encoded = row.instruction.bytes();
            if index == 0 && !encoded.is_empty() && self.editing_caret_visible() {
                let nibble = usize::from(
                    self.entry_column == EntryColumn::Hex && self.pending_nibble.is_some(),
                );
                set_body_caret(frame, body, location.len() + branch.len() + nibble, 0);
            }
            let mut spans = vec![
                Span::styled(location, selected),
                Span::styled(branch, BASE.fg(Color::Yellow)),
            ];
            let shortened = encoded.len() > 8;
            let display = &encoded[..encoded.len().min(if shortened { 7 } else { 8 })];
            for (column, byte) in display.iter().enumerate() {
                let position = row.offset.get().checked_add(column as u64);
                let text = if index == 0 && column == 0 {
                    format_hex_byte(*byte, self.pending_nibble)
                } else {
                    format!("{byte:02X}")
                };
                let style = if index == 0 && column == 0 && self.editor.is_editing() {
                    byte_style
                } else {
                    block_style(block, position, byte_style)
                };
                spans.push(Span::styled(text, style));
            }
            if shortened {
                spans.push(Span::styled(".. ", byte_style));
            } else {
                spans.push(Span::styled(" ".repeat(17 - display.len() * 2), byte_style));
            }
            spans.push(Span::raw(format!(
                "{state}{}",
                escaped_text(&row.instruction.text)
            )));
            lines.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(lines).style(BASE), body);
    }

    fn render_bytes(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let body = Rect::new(area.x, area.y + 2, area.width, area.height - 4);
        if self.reopen_required {
            frame.render_widget(
                Paragraph::new("<file was replaced; exit and reopen to verify its saved state>")
                    .style(BASE),
                body,
            );
            return;
        }
        if self.editor.document().is_empty() {
            frame.render_widget(
                Paragraph::new("<empty file - overwrite requires existing bytes>").style(BASE),
                body,
            );
            return;
        }
        let remaining = self.editor.document().len().saturating_sub(self.top);
        let wanted = (self.visible_rows * self.bytes_per_row).min(MAX_READ_BYTES);
        let count = remaining.min(wanted as u64) as usize;
        let mut bytes = vec![0; count];
        let count = match self
            .editor
            .document_mut()
            .read_at(FileOffset::new(self.top), &mut bytes)
        {
            Ok(count) => count,
            Err(error) => {
                self.notice(format!("Read failed: {error}"));
                frame.render_widget(Paragraph::new("<bytes unavailable>").style(BASE), body);
                return;
            }
        };
        if self.editing_caret_visible() {
            let relative = self.editor.cursor().get().saturating_sub(self.top);
            if relative < count as u64 {
                let index = relative as usize % self.bytes_per_row;
                let row = relative as usize / self.bytes_per_row;
                let column = match (self.editor.mode(), self.entry_column) {
                    (ViewMode::Text, _) => ADDRESS_COLUMNS + index,
                    (_, EntryColumn::Ascii) => ascii_column(self.bytes_per_row) + index,
                    (_, EntryColumn::Hex) => {
                        hex_column(index) + usize::from(self.pending_nibble.is_some())
                    }
                };
                set_body_caret(frame, body, column, row);
            }
        }
        let lines = bytes[..count]
            .chunks(self.bytes_per_row)
            .enumerate()
            .map(|(index, bytes)| {
                let offset = self.top.saturating_add((index * self.bytes_per_row) as u64);
                self.byte_line(offset, bytes)
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines).style(BASE), body);
    }

    fn byte_line(&self, offset: u64, bytes: &[u8]) -> Line<'static> {
        let mut spans = vec![Span::styled(format!("{offset:016X}  "), ADDRESS)];
        let block = self
            .editor
            .block_selection()
            .map(|selection| selection.range);
        if matches!(self.editor.mode(), ViewMode::Hex | ViewMode::Code) {
            for index in 0..self.bytes_per_row {
                if index > 0 && index % 2 == 0 {
                    spans.push(Span::raw(" "));
                }
                let position = offset.checked_add(index as u64);
                let current = position == Some(self.editor.cursor().get());
                let text = match bytes.get(index) {
                    Some(byte) => format_hex_byte(*byte, self.pending_nibble.filter(|_| current)),
                    None => "  ".into(),
                };
                let style = if current && index < bytes.len() {
                    if self.editor.is_editing() && self.entry_column == EntryColumn::Hex {
                        ACTIVE
                    } else {
                        SELECTED
                    }
                } else {
                    BASE
                };
                let style = if current || index >= bytes.len() {
                    style
                } else {
                    block_style(block, position, style)
                };
                spans.push(Span::styled(text, style));
            }
            spans.push(Span::raw("  "));
        }
        for (index, byte) in bytes.iter().enumerate() {
            let position = offset.checked_add(index as u64);
            let current = position == Some(self.editor.cursor().get());
            let style = if current {
                if self.editor.is_editing() && self.entry_column == EntryColumn::Ascii {
                    ACTIVE
                } else {
                    SELECTED
                }
            } else {
                BASE
            };
            let style = if current {
                style
            } else {
                block_style(block, position, style)
            };
            spans.push(Span::styled(printable(*byte).to_string(), style));
        }
        Line::from(spans)
    }

    fn render_function_bar(&self, frame: &mut Frame<'_>, area: Rect) {
        if matches!(self.dialog, Some(Dialog::Calculator)) {
            let mut labels = [""; 12];
            labels[0] = "Help";
            render_full_function_bar(frame, area, &labels, &[]);
            return;
        }
        let labels = if self.editor.is_searching() {
            Some([""; 10])
        } else {
            match self.dialog.as_ref() {
                Some(Dialog::Mode(_)) => {
                    Some(["Text", "Hex", "Decode", "Next", "", "", "", "", "", ""])
                }
                Some(Dialog::LineFeed(_)) => {
                    Some(["CR/LF", "CR", "LF", "NULL", "", "Next", "", "", "", ""])
                }
                Some(Dialog::Search(_)) => {
                    Some(["Help", "Direct", "", "Area", "GoLast", "", "", "", "", ""])
                }
                Some(Dialog::SearchNotice(_)) => Some([""; 10]),
                Some(Dialog::Input(input)) if input.kind == PromptKind::Goto => Some([""; 10]),
                _ => None,
            }
        };
        if let Some(labels) = labels {
            render_function_slots(frame, area, &labels, &[]);
            return;
        }
        let contextual = if self.reopen_required {
            Some(" File replaced | Esc exits | Reopen before further edits ")
        } else if self.export_in_progress {
            Some(" Staging block output | Esc cancel ")
        } else {
            match self.dialog.as_ref() {
                Some(Dialog::Analysis(dialog)) if dialog.page == super::AnalysisPage::Profiles => {
                    Some(
                        " 0..6 / Arrows select | Enter apply | L/B/E endian | R raw | V base | Esc ",
                    )
                }
                Some(Dialog::Analysis(dialog)) if dialog.page.is_symbols() => {
                    Some(" F7 imports | F9 exports | N symbols | Enter target | R record | Esc ")
                }
                Some(Dialog::Analysis(_)) => {
                    Some(" F5 entry | F6 regions | F7 imports | F9 exports | N symbols | Esc ")
                }
                Some(Dialog::Input(input)) if input.kind == PromptKind::RawBase => {
                    Some(" Enter sets raw base | Ctrl+U clear | Esc cancel ")
                }
                Some(Dialog::BlockWrite(_)) => {
                    Some(" Tab field | Enter accept | Ctrl+U clear | Esc cancel ")
                }
                _ => None,
            }
        };
        if let Some(text) = contextual {
            frame.render_widget(Paragraph::new(text).style(HEADER), area);
            return;
        }
        if self.editor.mode() == ViewMode::Hex {
            let (labels, disabled): ([_; 12], &[_]) = if self.editor.is_editing() {
                (
                    [
                        "Help", "", "Undo", "Byte", "Word", "Dword", "Crypt", "Xor", "Update",
                        "Trunc", "", "",
                    ],
                    &[4, 5, 6, 7, 8, 10],
                )
            } else {
                (
                    [
                        "Help", "PutBlk", "Edit", "Mode", "Goto", "DatRef", "Search", "Header",
                        "Files", "Quit", "Hem", "Names",
                    ],
                    &[6, 10, 11, 12],
                )
            };
            render_full_function_bar(frame, area, &labels, disabled);
            return;
        }
        if self.text_navigation() {
            let labels = [
                "Help",
                self.text_wrap_label(),
                "",
                "Mode",
                "Goto",
                "LnFeed",
                "Search",
                "Table",
                "Files",
                "Quit",
            ];
            render_function_slots(frame, area, &labels, &[8, 10]);
            return;
        }
        let labels = if self.editor.is_editing() {
            [
                "Help", "Asm-", "Rest", "--", "--", "--", "--", "--", "Save", "Trunc-",
            ]
        } else {
            let put = if self.editor.mode() != ViewMode::Text
                && self
                    .editor
                    .block_selection()
                    .is_some_and(|block| !block.is_marking)
            {
                "PutBlk"
            } else {
                "--"
            };
            [
                "Help", put, "Edit", "Mode", "Goto", "Refs-", "Search", "Header", "Files", "--",
            ]
        };
        render_function_slots(frame, area, &labels, &[]);
    }
}

pub(super) fn render_full_function_bar(
    frame: &mut Frame<'_>,
    area: Rect,
    labels: &[&str; 12],
    disabled: &[usize],
) {
    let slots = (area.width / 8).clamp(10, 12);
    render_function_slots(frame, area, &labels[..usize::from(slots)], disabled);
    let tail = area.width.saturating_sub(slots * 8);
    if tail != 0 {
        frame.render_widget(
            Paragraph::new("▒".repeat(usize::from(tail))).style(FOCUSED_ROW),
            Rect::new(area.right() - tail, area.y, tail, 1),
        );
    }
}

fn render_function_slots(frame: &mut Frame<'_>, area: Rect, labels: &[&str], disabled: &[usize]) {
    let mut spans = Vec::with_capacity(labels.len() * 2);
    for (index, label) in labels.iter().enumerate() {
        spans.push(Span::styled(
            format!("{:>2}", index + 1),
            Style::new().fg(Color::Gray).bg(Color::Black),
        ));
        let style = if label.contains('-') || disabled.contains(&(index + 1)) {
            HEADER.fg(Color::DarkGray)
        } else {
            HEADER
        };
        spans.push(Span::styled(format!("{label:<6}"), style));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(HEADER), area);
}

fn hex_column(index: usize) -> usize {
    ADDRESS_COLUMNS + index * 2 + index / 2
}

fn ascii_column(bytes_per_row: usize) -> usize {
    ADDRESS_COLUMNS + bytes_per_row * 2 + (bytes_per_row - 1) / 2 + 2
}

pub(super) fn set_body_caret(frame: &mut Frame<'_>, area: Rect, column: usize, row: usize) {
    if column < usize::from(area.width) && row < usize::from(area.height) {
        frame.set_cursor_position((area.x + column as u16, area.y + row as u16));
    }
}

fn block_style(block: Option<FileRange>, position: Option<u64>, fallback: Style) -> Style {
    match (block, position) {
        (Some(range), Some(position)) if range.contains(FileOffset::new(position)) => MARK,
        _ => fallback,
    }
}

fn format_hex_byte(byte: u8, pending: Option<u8>) -> String {
    match pending {
        Some(high) => format!("{high:X}_"),
        None => format!("{byte:02X}"),
    }
}

fn row(area: Rect, relative_y: u16) -> Rect {
    Rect::new(area.x, area.y.saturating_add(relative_y), area.width, 1)
}

pub(super) fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

/// Draw an inset double frame with a shadow two columns right and one row down. Keep the
/// function bar outside the surface, including on cramped terminals.
pub(super) fn dialog_surface<'a>(
    frame: &mut Frame<'_>,
    area: Rect,
    width: u16,
    height: u16,
    title: impl Into<Line<'a>>,
    style: Style,
) -> Rect {
    draw_dialog(
        frame,
        area,
        width,
        height,
        title.into(),
        style,
        DialogFrame::Default,
    )
}

pub(super) fn dialog_surface_with_tinted_shadow<'a>(
    frame: &mut Frame<'_>,
    area: Rect,
    width: u16,
    height: u16,
    title: impl Into<Line<'a>>,
    style: Style,
) -> Rect {
    draw_dialog(
        frame,
        area,
        width,
        height,
        title.into(),
        style,
        DialogFrame::TintedShadow,
    )
}

enum DialogFrame {
    Default,
    WithoutClose,
    TintedShadow,
}

fn draw_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    width: u16,
    height: u16,
    title: Line<'_>,
    style: Style,
    frame_style: DialogFrame,
) -> Rect {
    let available = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    let surface = popup(area, width, height.min(available.height));
    let shadow = Rect::new(
        surface.x.saturating_add(2),
        surface.y.saturating_add(1),
        surface.width,
        surface.height,
    )
    .intersection(available);
    if !matches!(frame_style, DialogFrame::TintedShadow) {
        frame.render_widget(Clear, shadow);
    }
    frame.render_widget(
        Block::default().style(Style::new().bg(Color::Black)),
        shadow,
    );
    frame.render_widget(Clear, surface);
    frame.render_widget(Block::default().style(style), surface);
    if !matches!(frame_style, DialogFrame::WithoutClose)
        && surface.width >= 3
        && surface.height >= 5
    {
        frame.render_widget(
            Paragraph::new("[x]").style(style.fg(Color::DarkGray)),
            Rect::new(surface.right() - 3, surface.y, 3, 1),
        );
    }

    let inset = surface.inner(Margin::new(
        if surface.width >= 27
            || (surface.width >= 6 && matches!(frame_style, DialogFrame::TintedShadow))
        {
            2
        } else {
            1
        },
        u16::from(surface.height >= 5),
    ));
    let border = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .title(title)
        .style(style);
    let content = border.inner(inset);
    frame.render_widget(border, inset);
    content
}

fn render_mode_picker(frame: &mut Frame<'_>, area: Rect, selected: ModeChoice) {
    let content = dialog_surface(
        frame,
        area,
        29,
        7,
        Line::from(" Select mode ").centered(),
        HEADER,
    );
    let content = content.inner(Margin::new(1, 0));
    let selected: usize = match selected {
        ModeChoice::Text => 0,
        ModeChoice::Hex => 1,
        ModeChoice::Code => 2,
    };
    let height = usize::from(content.height);
    let first = selected.saturating_sub(height.saturating_sub(1));
    for (line, label) in ["Text", "Hex", "Decode"]
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
    {
        let style = if line == selected {
            FOCUSED_ROW
        } else {
            HEADER.fg(Color::Blue)
        };
        let label_column = content.width.saturating_sub(5) / 2;
        frame.render_widget(
            Paragraph::new(format!(
                "{:width$}{label}",
                "",
                width = usize::from(label_column)
            ))
            .style(style),
            row(content, (line - first) as u16),
        );
    }
}

fn render_prompt(frame: &mut Frame<'_>, area: Rect, input: &InputDialog) {
    if input.kind == PromptKind::Goto {
        let field = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(1).min(24),
            1,
        );
        render_patterned_field(
            frame,
            field,
            &input.input,
            BASE.fg(Color::LightYellow),
            true,
        );
        if let Some(error) = input.error.as_deref() {
            frame.render_widget(Paragraph::new(error).style(PANEL), row(area, 2));
        }
        return;
    }
    let content = dialog_surface(frame, area, 66, 7, " Raw virtual base ", PANEL);
    let label = " Base ";
    let hint = "Absolute hexadecimal or decimal with suffix t.";
    if content.width <= label.len() as u16 || content.height == 0 {
        return;
    }
    let label_width = label.len() as u16;
    frame.render_widget(Paragraph::new(label).style(PANEL), row(content, 0));
    let field = Rect::new(
        content.x + label_width,
        content.y,
        content.width - label_width,
        1,
    );
    render_input_field(frame, field, &input.input, true);
    if content.height > 1 {
        frame.render_widget(
            Paragraph::new(input.error.as_deref().unwrap_or(hint)).style(PANEL),
            row(content, 1),
        );
    }
}

fn render_input_field(frame: &mut Frame<'_>, field: Rect, input: &str, active: bool) {
    if field.width == 0 {
        return;
    }
    let entry = escaped_text(input);
    let tail = visible_tail(&entry, field.width.saturating_sub(u16::from(active)));
    let style = if active { SELECTED } else { PANEL };
    frame.render_widget(Paragraph::new(tail).style(style), field);
    if active {
        frame.set_cursor_position((field.x + Span::raw(tail).width() as u16, field.y));
    }
}

fn render_search_notice(frame: &mut Frame<'_>, area: Rect, message: &str) {
    let content = dialog_surface(frame, area, 48, 5, "", ERROR_PANEL);
    let offset = content.height.saturating_sub(1) / 2;
    frame.render_widget(
        Paragraph::new(message).centered().style(ERROR_PANEL),
        row(content, offset),
    );
}

fn render_search(frame: &mut Frame<'_>, area: Rect, search: &SearchDialog) {
    let direction = match search.direction {
        SearchDirection::Forward => "Forward",
        SearchDirection::Backward => "Backward",
    };
    let panel = PANEL;
    let title = {
        let scope = if search.scope.is_some() {
            "Block"
        } else {
            "File"
        };
        let case = match search.field {
            SearchField::Text => "noCase",
            SearchField::Hex => "Case  ",
        };
        format!("══[{direction} •{scope} •{case}•Even]")
    };
    let content = dialog_surface(frame, area, 75, 7, title, panel);

    let alternate = search
        .alternate_input()
        .unwrap_or_else(|_| "<unavailable>".into());
    let hex_row = if content.height >= 3 { 2 } else { 1 };
    for (index, field, label) in [
        (0, SearchField::Text, " ASCII: "),
        (hex_row, SearchField::Hex, "   Hex: "),
    ] {
        if index >= content.height {
            break;
        }
        let active = field == search.field;
        let input = if active { &search.input } else { &alternate };
        let label_width = label.len() as u16;
        frame.render_widget(Paragraph::new(label).style(panel), row(content, index));
        let preferred_width = match field {
            SearchField::Text => 20,
            SearchField::Hex => 60,
        };
        let width = content
            .width
            .saturating_sub(label_width + 1)
            .min(preferred_width);
        let field = Rect::new(content.x + label_width, content.y + index, width, 1);
        let style = if active { ACTIVE_SEARCH_FIELD } else { PANEL };
        render_patterned_field(frame, field, input, style, active);
    }

    if let Some(error) = search.error.as_deref().filter(|_| content.height >= 3) {
        frame.render_widget(Paragraph::new(error).style(panel), row(content, 1));
    }
}

fn render_patterned_field(
    frame: &mut Frame<'_>,
    field: Rect,
    input: &str,
    style: Style,
    active: bool,
) {
    if field.width == 0 {
        return;
    }
    let entry = escaped_text(input);
    let tail = visible_tail(&entry, field.width.saturating_sub(u16::from(active)));
    let width = Span::raw(tail).width() as u16;
    let fill = "░".repeat(usize::from(field.width - width));
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::raw(tail), Span::raw(fill)])).style(style),
        field,
    );
    if active {
        frame.set_cursor_position((field.x + width, field.y));
    }
}

fn render_block_write(frame: &mut Frame<'_>, area: Rect, dialog: &BlockWrite) {
    let panel = PANEL;
    let content = dialog_surface(frame, area, 68, 11, " Write block ", panel);
    let first = dialog.range.offset.get();
    let last = first + dialog.range.len - 1;
    let length = dialog.range.len;
    let lines = [
        format!("Block  {first:08X}–{last:08X}"),
        format!("Length {length:08X}/{length}"),
        "File".into(),
        "Offset".into(),
        "Format".into(),
        dialog
            .message
            .as_deref()
            .unwrap_or("Blank offset starts at zero. * appends.")
            .into(),
    ];
    for (index, text) in lines
        .into_iter()
        .take(usize::from(content.height))
        .enumerate()
    {
        frame.render_widget(
            Paragraph::new(text).style(panel),
            row(content, index as u16),
        );
    }
    for (index, input, focus) in [
        (2, dialog.file.as_str(), BlockField::File),
        (3, dialog.offset.as_str(), BlockField::Offset),
        (4, "Raw bytes", BlockField::Format),
    ] {
        if content.height <= index || content.width <= 8 {
            continue;
        }
        let field = Rect::new(content.x + 8, content.y + index, content.width - 8, 1);
        let focused = focus == dialog.focus;
        let style = if focused { SELECTED } else { panel };
        let input = escaped_text(input);
        let tail = visible_tail(&input, field.width - 1);
        frame.render_widget(Paragraph::new(tail).style(style), field);
        if focused && focus != BlockField::Format {
            frame.set_cursor_position((field.x + Span::raw(tail).width() as u16, field.y));
        }
    }
}

fn visible_tail(input: &str, columns: u16) -> &str {
    let mut available = usize::from(columns);
    let mut start = input.len();
    for (index, character) in input.char_indices().rev() {
        let mut utf8 = [0; 4];
        let width = Span::raw(character.encode_utf8(&mut utf8) as &str).width();
        let Some(remaining) = available.checked_sub(width) else {
            break;
        };
        available = remaining;
        start = index;
    }
    &input[start..]
}
