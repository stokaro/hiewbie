use hiewbie_core::formats::FileRange;
use hiewbie_core::instructions::DecodeStatus;
use hiewbie_core::{
    ByteSource, FileOffset, MAX_CODE_ROWS, MAX_READ_BYTES, SearchDirection, ViewMode,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use super::block::{BlockField, BlockWrite};
use super::search::{SearchDialog, SearchField};
use super::{
    Dialog, EntryColumn, InputDialog, ModeChoice, PromptKind, Ui, escaped_text, printable,
};

pub(super) const BASE: Style = Style::new().fg(Color::Gray).bg(Color::Blue);
const ADDRESS: Style = Style::new().fg(Color::Cyan).bg(Color::Blue);
const SELECTED: Style = Style::new().fg(Color::Black).bg(Color::Cyan);
const MARK: Style = Style::new().fg(Color::Yellow).bg(Color::Magenta);
const ACTIVE: Style = Style::new().fg(Color::Black).bg(Color::Yellow);
pub(super) const HEADER: Style = Style::new().fg(Color::Black).bg(Color::Cyan);

impl<S: ByteSource> Ui<S> {
    pub(super) fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        if area.width == 0 || area.height == 0 {
            return;
        }
        if self.editor.mode() == ViewMode::Code || matches!(self.dialog, Some(Dialog::Analysis(_)))
        {
            self.editor.refresh_analysis();
        }
        frame.render_widget(Block::default().style(BASE), area);
        let header = format!(" hiewbie {} | {}", env!("CARGO_PKG_VERSION"), self.label);
        frame.render_widget(Paragraph::new(header).style(HEADER), row(area, 0));
        self.update_viewport(area);

        if area.width < 23 || area.height < 5 {
            if area.height > 1 {
                frame.render_widget(
                    Paragraph::new("Resize: at least 23 x 5").style(BASE),
                    row(area, 1),
                );
            }
            return;
        }

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
        frame.render_widget(Paragraph::new(status).style(ADDRESS), row(area, 1));

        if self.editor.mode() == ViewMode::Code {
            self.render_code(frame, area);
        } else {
            self.render_bytes(frame, area);
        }
        self.render_status_message(frame, row(area, area.height - 2));
        self.render_function_bar(frame, row(area, area.height - 1));
        match self.dialog.as_ref() {
            Some(Dialog::Help) => render_help(frame, area),
            Some(Dialog::Input(input)) => render_prompt(frame, area, input),
            Some(Dialog::Search(search)) => render_search(frame, area, search),
            Some(Dialog::BlockWrite(dialog)) => {
                if !self.export_in_progress {
                    render_block_write(frame, area, dialog);
                }
            }
            Some(Dialog::Mode(selected)) => render_mode_picker(frame, area, *selected),
            Some(Dialog::Analysis(dialog)) => self.render_analysis_dialog(frame, area, dialog),
            None => {}
        }
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
            ViewMode::Hex => [64, 32, 16, 8, 4, 2, 1]
                .into_iter()
                .find(|count| 20 + 3 * count + (count - 1) / 2 <= area.width as usize)
                .unwrap_or(1),
            ViewMode::Code => 1,
        };
        self.visible_rows = (area.height.saturating_sub(4) as usize)
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
            let mut spans = vec![
                Span::styled(
                    format!(
                        "{:08X} {domain}{:016X} ",
                        row.offset.get(),
                        row.instruction.address.get()
                    ),
                    selected,
                ),
                Span::styled(format!("{branch} "), BASE.fg(Color::Yellow)),
            ];
            let encoded = row.instruction.bytes();
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
        let contextual = if self.reopen_required {
            Some(" File replaced | Esc exits | Reopen before further edits ")
        } else if self.editor.is_searching() {
            Some(" Searching in bounded steps | Esc cancel ")
        } else if self.export_in_progress {
            Some(" Staging block output | Esc cancel ")
        } else {
            match self.dialog.as_ref() {
                Some(Dialog::Help) => Some(" F1 / Enter / Esc close help "),
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
                Some(Dialog::Search(search)) if search.help => {
                    Some(" F1 / Enter / Esc close search help ")
                }
                Some(Dialog::Search(_))
                    if self.editor.block_selection().is_some_and(|b| !b.is_marking) =>
                {
                    Some(
                        " F1 Help  F2 Direct  F4 Area  F5 GoLast  Tab field  Enter find  Esc cancel ",
                    )
                }
                Some(Dialog::Search(_)) => {
                    Some(" F1 Help   F2 Direct   F5 GoLast   Tab field   Enter find   Esc cancel ")
                }
                Some(Dialog::Input(_)) => Some(" Enter goto | Ctrl+U clear | Esc cancel "),
                Some(Dialog::Mode(_)) => Some(" Up/Down choose | Enter select | Esc cancel "),
                Some(Dialog::BlockWrite(_)) => {
                    Some(" Tab field | Enter accept | Ctrl+U clear | Esc cancel ")
                }
                None => None,
            }
        };
        if let Some(text) = contextual {
            frame.render_widget(Paragraph::new(text).style(HEADER), area);
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
                "Help", put, "Edit", "Mode", "Goto", "Refs-", "Search", "Header", "Open-", "--",
            ]
        };
        let mut spans = Vec::with_capacity(20);
        for (index, label) in labels.into_iter().enumerate() {
            spans.push(Span::styled(
                format!("{:>2}", index + 1),
                BASE.fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
            let available = !label.contains('-');
            let style = if available {
                HEADER
            } else {
                HEADER.fg(Color::DarkGray)
            };
            spans.push(Span::styled(format!("{label:<6}"), style));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)).style(HEADER), area);
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

fn render_help(frame: &mut Frame<'_>, area: Rect) {
    let popup = popup(area, 80, 25);
    let lines = [
        "Hex, ASCII Text, and Code share one document and file cursor.",
        "View  Enter cycle; F4 select mode; F3/Ctrl+E edit; Esc quit",
        "      F5/Ctrl+G goto; F7/Ctrl+F search; F8/Ctrl+O headers",
        "      Shift+F7 / Ctrl+Enter / Ctrl+R repeat (same direction, no wrap)",
        "Code  1..9 direct branch; Backspace return; / resynchronize",
        "      Up/Down instructions; Left/Right bytes; PgUp/PgDn pages",
        "      Unknown backward boundaries use one byte/ISA alignment.",
        "      Ctrl+F1/Ctrl+B x86 bitness; Shift+F1/Ctrl+A decoder",
        "Profile 0 Auto; 1..3 x86 16/32/64; 4 ARM; 5 Thumb; 6 AArch64",
        "      L/B/E little/big/automatic endian; R raw; V raw base",
        "      Big-endian decode unsupported; raw bytes remain available.",
        "Headers F5 entry; F6 regions; F7 imports; F9 exports; N symbols",
        "Edit  F9/Ctrl+S save; Esc discard; Tab hex/ASCII; Ctrl+Z/Y undo/redo",
        "      F3 restores current byte; Backspace restores previous byte.",
        "      Two digits commit one byte; Backspace cancels a digit first.",
        "      Code editing changes bytes; textual assembly is planned.",
        "Goto  Hex default, decimal suffix t, +/- relative; .address is VA",
        "      V: image VA; R: raw base address; F: unmapped file offset",
        "Find  Tab hex/UTF-8 text; Esc cancels scan; Ctrl+Home/End file edges",
        "Block * start/finish/clear; [ / ] endpoints; Alt+* resize",
        "      Ctrl+*/Ctrl+K select all; F4 in Search toggles File/Block",
        "      F2 exports a completed block; * in the offset field appends",
        "Paste rejected. Global references and multi-file planned.",
        "F9 in view means Open (planned), never Save. '-' means planned.",
        "Legacy terminals may intercept keys; use the Ctrl alternatives.",
        "Close help with F1, Enter, or Esc.",
    ]
    .into_iter()
    .map(Line::raw)
    .collect::<Vec<_>>();
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).style(HEADER).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" hiewbie help ")
                .style(HEADER),
        ),
        popup,
    );
}

fn render_mode_picker(frame: &mut Frame<'_>, area: Rect, selected: ModeChoice) {
    let popup = popup(area, 34, 7);
    let mut lines = [
        (ModeChoice::Text, "Text"),
        (ModeChoice::Hex, "Hex"),
        (ModeChoice::Code, "Code"),
    ]
    .into_iter()
    .map(|(choice, label)| {
        let (marker, style) = if choice == selected {
            (">", ACTIVE)
        } else {
            (" ", HEADER)
        };
        Line::styled(format!("{marker} {label}"), style)
    })
    .collect::<Vec<_>>();
    lines.push(Line::raw(""));
    lines.push(Line::raw("Enter selects; Esc cancels"));
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines)
            .style(HEADER)
            .block(Block::default().borders(Borders::ALL).title(" View mode ")),
        popup,
    );
}

fn render_prompt(frame: &mut Frame<'_>, area: Rect, input: &InputDialog) {
    let popup = popup(area, 74, 8);
    let (title, hint) = match input.kind {
        PromptKind::Goto => (
            " Goto file offset or virtual address ",
            "Hex default; decimal suffix t; +/- relative; .address maps a VA.",
        ),
        PromptKind::RawBase => (
            " Raw virtual base ",
            "Absolute hexadecimal or decimal suffix t. Enables raw address mapping.",
        ),
    };
    // Display the tail of long input; all input is retained for validation.
    let available = popup.width.saturating_sub(6) as usize;
    let escaped = escaped_text(&input.input);
    let tail = escaped
        .chars()
        .rev()
        .take(available)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let lines = vec![
        Line::raw(hint),
        Line::raw(""),
        Line::styled(
            format!("> {tail}_"),
            Style::new().fg(Color::Yellow).bg(Color::Blue),
        ),
        Line::raw(
            input
                .error
                .as_deref()
                .unwrap_or("Enter applies; Ctrl+U clears; Esc cancels"),
        ),
    ];
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines)
            .style(HEADER)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .style(HEADER),
            ),
        popup,
    );
}

fn render_search(frame: &mut Frame<'_>, area: Rect, search: &SearchDialog) {
    // Keep the main function bar visible even on the smallest usable terminal.
    let overlay = Rect::new(area.x, area.y, area.width, area.height - 1);
    let popup = popup(overlay, 72, 7);
    let direction = match search.direction {
        SearchDirection::Forward => "Forward",
        SearchDirection::Backward => "Backward",
    };
    let panel = Style::new().fg(Color::Black).bg(Color::Gray);
    let title = if search.help {
        " Search help ".into()
    } else {
        let scope = if search.scope.is_some() {
            "Block"
        } else {
            "File"
        };
        format!(" Search | {direction} | {scope} ")
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(panel);
    let content = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);
    if search.help {
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw("Tab or Up/Down switches field. Ctrl+U clears the query."),
                Line::raw("F2 reverses the scan. F5 goes to the last match."),
                Line::raw("F4 selects the file or a completed block."),
                Line::raw("UTF-8 folds ASCII letters only. Hex matches exact bytes."),
            ])
            .style(panel),
            content,
        );
        return;
    }

    let alternate = search
        .alternate_input()
        .unwrap_or_else(|_| "<unavailable>".into());
    for (index, (field, label)) in [(SearchField::Text, "UTF-8 "), (SearchField::Hex, "Hex   ")]
        .into_iter()
        .enumerate()
    {
        if index as u16 >= content.height {
            break;
        }
        let active = field == search.field;
        let input = if active { &search.input } else { &alternate };
        let entry = escaped_text(input);
        let field_width = content.width.saturating_sub(label.len() as u16);
        let tail = visible_tail(&entry, field_width.saturating_sub(1));
        let y = content.y + index as u16;
        frame.render_widget(
            Paragraph::new(label).style(panel),
            row(content, index as u16),
        );
        let field_area = Rect::new(content.x + label.len() as u16, y, field_width, 1);
        let field_style = if active { SELECTED } else { panel };
        frame.render_widget(Paragraph::new(tail).style(field_style), field_area);
        if active && field_width > 0 {
            let cursor = Span::raw(tail).width() as u16;
            frame.set_cursor_position((field_area.x + cursor, y));
        }
    }

    if content.height >= 3 {
        let hint = search.error.as_deref().unwrap_or(match search.field {
            SearchField::Text => "UTF-8 matches with ASCII case folding.",
            SearchField::Hex => "Hex matches exact bytes. Use two digits per byte.",
        });
        frame.render_widget(Paragraph::new(hint).style(panel), row(content, 2));
    }
}

fn render_block_write(frame: &mut Frame<'_>, area: Rect, dialog: &BlockWrite) {
    let overlay = Rect::new(area.x, area.y, area.width, area.height - 1);
    let popup = popup(overlay, 68, 9);
    let panel = Style::new().fg(Color::Black).bg(Color::Gray);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Write block ")
        .style(panel);
    let content = block.inner(popup);
    let first = dialog.range.offset.get();
    let last = first + dialog.range.len - 1;
    let length = dialog.range.len;
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);
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
