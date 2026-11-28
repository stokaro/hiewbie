use hiewbie_core::instructions::DecodeStatus;
use hiewbie_core::{ByteSource, FileOffset, MAX_CODE_ROWS, MAX_READ_BYTES, ViewMode};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use super::{Dialog, EntryColumn, InputDialog, PromptKind, Ui, escaped_text, printable};

pub(super) const BASE: Style = Style::new().fg(Color::Gray).bg(Color::Blue);
const ADDRESS: Style = Style::new().fg(Color::Cyan).bg(Color::Blue);
const SELECTED: Style = Style::new().fg(Color::Black).bg(Color::Cyan);
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
        frame.render_widget(
            Paragraph::new(self.message.as_str()).style(BASE.fg(Color::Yellow)),
            row(area, area.height - 2),
        );
        self.render_function_bar(frame, row(area, area.height - 1));
        match self.dialog.as_ref() {
            Some(Dialog::Help) => render_help(frame, area),
            Some(Dialog::Input(input)) => render_prompt(frame, area, input),
            Some(Dialog::Analysis(dialog)) => self.render_analysis_dialog(frame, area, dialog),
            None => {}
        }
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
        for (index, row) in rows.iter().enumerate() {
            let domain = if raw {
                "R:"
            } else if row.mapped {
                "V:"
            } else {
                "F:"
            };
            let mut encoded = row
                .instruction
                .bytes()
                .iter()
                .take(8)
                .map(|byte| format!("{byte:02X}"))
                .collect::<String>();
            if row.instruction.bytes().len() > 8 {
                encoded.truncate(14);
                encoded.push_str("..");
            }
            if index == 0 && self.pending_nibble.is_some() && !encoded.is_empty() {
                encoded.replace_range(..2, &format!("{:X}_", self.pending_nibble.unwrap_or(0)));
            }
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
            lines.push(Line::from(vec![
                Span::styled(
                    format!(
                        "{:08X} {domain}{:016X} ",
                        row.offset.get(),
                        row.instruction.address.get()
                    ),
                    selected,
                ),
                Span::styled(format!("{branch} "), BASE.fg(Color::Yellow)),
                Span::styled(format!("{encoded:<16} "), byte_style),
                Span::raw(format!("{state}{}", escaped_text(&row.instruction.text))),
            ]));
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
        if matches!(self.editor.mode(), ViewMode::Hex | ViewMode::Code) {
            for index in 0..self.bytes_per_row {
                if index > 0 && index % 2 == 0 {
                    spans.push(Span::raw(" "));
                }
                let current = offset.checked_add(index as u64) == Some(self.editor.cursor().get());
                let text = match bytes.get(index) {
                    Some(_) if current && self.pending_nibble.is_some() => {
                        format!("{:X}_", self.pending_nibble.unwrap_or(0))
                    }
                    Some(byte) => format!("{byte:02X}"),
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
                spans.push(Span::styled(text, style));
            }
            spans.push(Span::raw("  "));
        }
        for (index, byte) in bytes.iter().enumerate() {
            let current = offset.checked_add(index as u64) == Some(self.editor.cursor().get());
            let style = if current {
                if self.editor.is_editing() && self.entry_column == EntryColumn::Ascii {
                    ACTIVE
                } else {
                    SELECTED
                }
            } else {
                BASE
            };
            spans.push(Span::styled(printable(*byte).to_string(), style));
        }
        Line::from(spans)
    }

    fn render_function_bar(&self, frame: &mut Frame<'_>, area: Rect) {
        let contextual = if self.reopen_required {
            Some(" File replaced | Esc exits | Reopen before further edits ")
        } else if self.search.is_some() {
            Some(" Searching in bounded steps | Esc cancel ")
        } else {
            match self.dialog.as_ref() {
                Some(Dialog::Help) => Some(" F1 / Enter / Esc close help "),
                Some(Dialog::Analysis(dialog)) if dialog.page == super::AnalysisPage::Profiles => {
                    Some(
                        " 0..6 / Arrows select | Enter apply | L/B/E endian | R raw | V base | Esc ",
                    )
                }
                Some(Dialog::Analysis(_)) => Some(
                    " F5 entry | F6 regions | F4 fields | S slices | Enter follow | Esc close ",
                ),
                Some(Dialog::Input(input)) if input.kind == PromptKind::RawBase => {
                    Some(" Enter sets raw base | Ctrl+U clear | Esc cancel ")
                }
                Some(Dialog::Input(input))
                    if matches!(input.kind, PromptKind::SearchHex | PromptKind::SearchText) =>
                {
                    Some(" Enter search | Tab hex/text | Ctrl+U clear | Esc cancel ")
                }
                Some(Dialog::Input(_)) => Some(" Enter goto | Ctrl+U clear | Esc cancel "),
                None => None,
            }
        };
        if let Some(text) = contextual {
            frame.render_widget(Paragraph::new(text).style(HEADER), area);
            return;
        }
        let labels = if self.editor.is_editing() {
            [
                "Help", "Asm-", "Rest-", "--", "--", "--", "--", "--", "Save", "Trunc-",
            ]
        } else {
            [
                "Help", "--", "Edit", "Mode", "Goto", "Refs-", "Search", "Header", "Open-", "--",
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
        "Hex, ASCII Text, and Code views share one document and file cursor.",
        "View  Enter/F4 modes  F3/Ctrl+E edit  F8/Ctrl+O headers  Esc quit",
        "      F5/Ctrl+G goto  F7/Ctrl+F search  Ctrl+Enter/Ctrl+R repeat",
        "Code  1..9 direct branch  Backspace return  / resynchronize",
        "      Up/Down instructions; Left/Right bytes; PgUp/PgDn pages",
        "      Unknown backward boundaries use one byte/ISA alignment.",
        "      Ctrl+F1/Ctrl+B x86 bitness; Shift+F1/Ctrl+A decoder",
        "Profile 0 Auto; 1..3 x86 16/32/64; 4 ARM; 5 Thumb; 6 AArch64",
        "      L/B/E little/big/automatic endian; R raw; V raw base",
        "      Big-endian decode unsupported; raw bytes remain available.",
        "Headers F5 entry; F6 regions; F4 fields; S slices; Enter follows",
        "Edit  F9/Ctrl+S save  Esc discard  Tab hex/ASCII  Ctrl+Z/Y undo/redo",
        "      Code edit changes bytes only; textual assembly is planned.",
        "      Two digits commit one byte; Backspace cancels a digit.",
        "Goto  Hex default, decimal suffix t, +/- relative; .address is VA",
        "      V: image VA; R: raw base address; F: unmapped file offset",
        "Find  Tab hex/UTF-8 text; forward, no wrap; Esc cancels scan",
        "Paste rejected. Ctrl+Home/End file edges; F1 closes help.",
        "Planned: textual assembly, global references, blocks, multi-file.",
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
        PromptKind::SearchHex => (
            " Search HEX (Tab: Text) ",
            "Pairs of hex digits, e.g. 48 65 6C 6C 6F. Forward search; no wrap.",
        ),
        PromptKind::SearchText => (
            " Search UTF-8 TEXT (Tab: Hex) ",
            "Literal UTF-8 bytes; case-sensitive. Forward search; no wrap.",
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
