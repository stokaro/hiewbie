use hiewbie_core::formats::{BinaryFormat, Executable, FileRange};
use hiewbie_core::instructions::DecodeMode;
use hiewbie_core::{BookmarkMark, ByteSource, FileOffset, MAX_READ_BYTES};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{EntryColumn, FileAccess, Ui, render};

pub(super) const BYTE_BODY: Style = Style::new().fg(Color::LightCyan).bg(Color::Blue);
const SELECTED: Style = Style::new().fg(Color::Blue).bg(Color::Gray);
const CHANGED: Style = BYTE_BODY.fg(Color::LightYellow);
const SCROLLBAR: Style = Style::new().fg(Color::Green).bg(Color::Black);

pub(super) struct HexLayout {
    address_width: usize,
    pub(super) bytes_per_row: usize,
}

impl HexLayout {
    pub(super) fn new(width: u16, len: u64, pe64: bool) -> Self {
        // PE32+ and large raw files keep the full value in the title. Expand
        // the row address when the complete line and two trailing cells fit.
        let address_width = if width >= 89 && (pe64 || len.saturating_sub(1) > u64::from(u32::MAX))
        {
            17
        } else {
            8
        };
        let bytes_per_row = [16, 8, 4, 2, 1]
            .into_iter()
            .find(|count| address_width + 4 * count + 6 <= usize::from(width))
            .unwrap_or(1);
        Self {
            address_width,
            bytes_per_row,
        }
    }

    fn hex_column(&self, index: usize) -> usize {
        self.address_width + 5 + index * 3
    }

    fn character_column(&self) -> usize {
        self.hex_column(self.bytes_per_row) + 1
    }
}

impl<S: ByteSource> Ui<S> {
    fn hex_pe(&self) -> Option<&Executable> {
        self.editor
            .executable()
            .filter(|image| matches!(image.format, BinaryFormat::Pe32 | BinaryFormat::Pe64))
    }

    fn hex_format(&self) -> Option<BinaryFormat> {
        if self.editor.is_editing() {
            self.edit_format
        } else {
            self.editor.executable().map(|image| image.format)
        }
    }

    pub(super) fn hex_layout(&self, width: u16) -> HexLayout {
        HexLayout::new(
            width,
            self.editor.document().len(),
            self.hex_format() == Some(BinaryFormat::Pe64),
        )
    }

    // The title always presents the full address. PE Hex editing uses file
    // offsets, and ambiguous or unbacked addresses also fall back to file offsets.
    pub(super) fn render_hex_title(&self, frame: &mut Frame<'_>, area: Rect, scan: Option<u64>) {
        let format = self.hex_format();
        let cursor = FileOffset::new(scan.unwrap_or(self.editor.cursor().get()));
        let image = self
            .hex_pe()
            .filter(|_| !self.editor.is_editing() && scan.is_none());
        let (address_marker, address) = byte_address(image, cursor);
        let wide = format == Some(BinaryFormat::Pe64)
            || self.editor.document().len().saturating_sub(1) > u64::from(u32::MAX)
            || address > u64::from(u32::MAX);
        let address = format!("{address_marker}{}", address_digits(address, wide));
        let mode = match format {
            Some(BinaryFormat::Pe32) => "PE",
            Some(BinaryFormat::Pe64) => "PE+",
            Some(BinaryFormat::Elf32 | BinaryFormat::Elf64) => "ELF",
            Some(BinaryFormat::MachO32 | BinaryFormat::MachO64) => "M-O",
            None => "",
        };
        let detail = format!("     {mode:>3}{address:>18}");
        self.render_byte_title(frame, area, &address, &detail, mode);
    }

    pub(super) fn render_text_title(&self, frame: &mut Frame<'_>, area: Rect, scan: Option<u64>) {
        let offset = scan.unwrap_or(self.editor.cursor().get());
        let wide = self.editor.document().len().saturating_sub(1) > u64::from(u32::MAX);
        let address = address_digits(offset, wide);
        let column = self
            .text
            .horizontal()
            .map_or_else(String::new, |column| column.to_string());
        let address_width = address.len().max(13);
        let column_width = 26 - address_width;
        let detail = format!("{column:>column_width$}{address:>address_width$}");
        self.render_byte_title(frame, area, &address, &detail, &column);
    }

    pub(super) fn render_code_title(&self, frame: &mut Frame<'_>, area: Rect) {
        let cursor = self.editor.cursor();
        let profile = self.editor.decode_context();
        let editing = self.editor.is_editing();
        let image = self.editor.executable().filter(|_| !editing);
        let (marker, address) = if !editing && profile.raw && profile.base.get() != 0 {
            profile
                .base
                .get()
                .checked_add(cursor.get())
                .map_or((' ', cursor.get()), |address| ('.', address))
        } else {
            byte_address(image, cursor)
        };
        let wide = self.hex_format() == Some(BinaryFormat::Pe64)
            || self.editor.document().len().saturating_sub(1) > u64::from(u32::MAX)
            || address > u64::from(u32::MAX);
        let address = format!("{marker}{}", address_digits(address, wide));
        let automatic = self.editor.decode_mode_override().is_none();
        let context = match (profile.mode, automatic) {
            (Some(DecodeMode::X86_16), true) => "a16",
            (Some(DecodeMode::X86_32), true) => "a32",
            (Some(DecodeMode::X86_64), true) => "a64",
            (Some(DecodeMode::X86_16), false) => "16",
            (Some(DecodeMode::X86_32), false) => "32",
            (Some(DecodeMode::X86_64), false) => "64",
            (Some(DecodeMode::Arm), _) => "ARM",
            (Some(DecodeMode::Thumb), _) => "THM",
            (Some(DecodeMode::Aarch64), _) => "A64",
            (None, _) => "",
        };
        let address_width = address.len().max(13);
        let context_width = 26 - address_width;
        let detail = match self.hex_format() {
            Some(BinaryFormat::Pe32) => format!("{context:>4}{:>4}{address:>18}", "PE"),
            Some(BinaryFormat::Pe64) => format!("{context:>4}{:>4}{address:>18}", "PE+"),
            _ => format!("{context:>context_width$}{address:>address_width$}"),
        };
        self.render_byte_title(frame, area, &address, &detail, context);
    }

    fn render_byte_title(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        address: &str,
        detail: &str,
        context: &str,
    ) {
        let access = match self.file_access {
            FileAccess::Read => "FRO",
            FileAccess::Write => "FWO",
            FileAccess::Updated => "FUO",
        };
        let bookmarks: String;
        let state = if self.reopen_required {
            "REOPEN  "
        } else if self.editor.is_editing() {
            "EDITMODE"
        } else {
            bookmarks = self
                .editor
                .bookmark_marks()
                .into_iter()
                .enumerate()
                .map(|(index, mark)| match mark {
                    BookmarkMark::Empty => '-',
                    BookmarkMark::Stored => char::from(b'1' + index as u8),
                    BookmarkMark::Current => '♦',
                })
                .collect();
            &bookmarks
        };
        if area.width < 63 {
            let title = format!(" {address} {access} {state} {context}");
            frame.render_widget(Paragraph::new(title).style(render::PANEL), area);
        } else {
            const STATUS_WIDTH: u16 = 58;
            let start = area.right() - STATUS_WIDTH;
            let caption = format!("hiewbie {}", env!("CARGO_PKG_VERSION"));
            let status = format!("↓{access} {state}{detail}│{caption:<18}");
            frame.render_widget(Paragraph::new("").style(render::PANEL), area);
            frame.render_widget(
                Paragraph::new(status).style(render::PANEL),
                Rect::new(start, area.y, STATUS_WIDTH, 1),
            );
            let label_width = start - area.x - 5;
            let label = if Span::raw(self.label.as_str()).width() <= usize::from(label_width) {
                self.label.as_str()
            } else {
                &self.short_label
            };
            frame.render_widget(
                Paragraph::new(label).style(render::PANEL),
                Rect::new(area.x + 5, area.y, label_width, 1),
            );
        }
        frame.render_widget(
            Paragraph::new("▲").style(SCROLLBAR),
            Rect::new(area.x, area.y, 1, 1),
        );
    }

    pub(super) fn render_hex(&mut self, frame: &mut Frame<'_>, body: Rect, layout: &HexLayout) {
        let length = self.editor.document().len();
        if self.reopen_required {
            frame.render_widget(
                Paragraph::new("<file was replaced; exit and reopen to verify its saved state>")
                    .style(BYTE_BODY),
                body,
            );
            return;
        }
        if length == 0 {
            let message = Rect::new(body.x + 1, body.y, body.width - 1, 1);
            frame.render_widget(
                Paragraph::new("<empty file - overwrite requires existing bytes>").style(BYTE_BODY),
                message,
            );
            render_scrollbar(frame, body, 0, 0, 0);
            return;
        }
        let wanted = (self.visible_rows * layout.bytes_per_row).min(MAX_READ_BYTES);
        let limit = length.saturating_sub(self.top).min(wanted as u64) as usize;
        let mut bytes = vec![0; limit];
        let count = match self
            .editor
            .document_mut()
            .read_at(FileOffset::new(self.top), &mut bytes)
        {
            Ok(count) => count,
            Err(error) => {
                self.notice(format!("Read failed: {error}"));
                frame.render_widget(Paragraph::new("<bytes unavailable>").style(BYTE_BODY), body);
                return;
            }
        };

        if self.editing_caret_visible() {
            let relative = self.editor.cursor().get().saturating_sub(self.top);
            if relative < count as u64 {
                let index = relative as usize % layout.bytes_per_row;
                let row = relative as usize / layout.bytes_per_row;
                let column = match self.entry_column {
                    EntryColumn::Ascii => layout.character_column() + index,
                    EntryColumn::Hex => {
                        layout.hex_column(index) + usize::from(self.pending_nibble.is_some())
                    }
                };
                render::set_body_caret(frame, body, column, row);
            }
        }
        let rows = bytes[..count]
            .chunks(layout.bytes_per_row)
            .enumerate()
            .map(|(row, bytes)| {
                let offset = self.top + (row * layout.bytes_per_row) as u64;
                self.hex_line(offset, bytes, layout)
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(rows).style(BYTE_BODY), body);
        render_scrollbar(frame, body, self.top, length, wanted as u64);
    }

    fn hex_line(&self, offset: u64, bytes: &[u8], layout: &HexLayout) -> Line<'static> {
        let editing = self.editor.is_editing();
        let image = self.hex_pe().filter(|_| !editing);
        let (marker, address) = byte_address(image, FileOffset::new(offset));
        let address = address_digits(address, layout.address_width == 17);
        let mut spans = vec![Span::raw(format!(" {marker}{address}:  "))];
        let mut characters = Vec::with_capacity(bytes.len());
        let block = self.editor.block_selection().map(|block| block.range);
        for index in 0..layout.bytes_per_row {
            let byte = bytes.get(index).copied();
            let delimiter = if index + 1 < layout.bytes_per_row && index % 4 == 3 {
                "-"
            } else {
                " "
            };
            if let Some(byte) = byte {
                let position = FileOffset::new(offset + index as u64);
                let current = position == self.editor.cursor();
                let pending = self.pending_nibble.filter(|_| current);
                let shown = pending.map_or(byte, |nibble| (nibble << 4) | (byte & 0x0F));
                let modified =
                    editing && (shown != byte || self.editor.document().is_byte_modified(position));
                let style = byte_style(editing, current, modified, block, position);
                spans.push(Span::styled(format!("{shown:02X}"), style));
                // Changes color the byte's following separator. Viewing
                // selection covers exactly the two digits and the glyph.
                spans.push(Span::styled(
                    delimiter,
                    if modified { style } else { BYTE_BODY },
                ));
                characters.push(Span::styled(pc_glyph(shown).to_string(), style));
            } else {
                spans.push(Span::raw("  "));
                spans.push(Span::raw(delimiter));
            }
        }
        spans.push(Span::raw(" "));
        spans.extend(characters);
        Line::from(spans)
    }
}

fn byte_style(
    editing: bool,
    current: bool,
    modified: bool,
    block: Option<FileRange>,
    position: FileOffset,
) -> Style {
    if !editing && current {
        SELECTED
    } else if block.is_some_and(|range| range.contains(position)) {
        render::MARK
    } else if modified {
        CHANGED
    } else {
        BYTE_BODY
    }
}

fn byte_address(image: Option<&Executable>, offset: FileOffset) -> (char, u64) {
    image
        .and_then(|image| image.file_to_va(offset).ok())
        .map_or((' ', offset.get()), |address| ('.', address.get()))
}

pub(super) fn address_digits(value: u64, wide: bool) -> String {
    if wide {
        format!("{:08X}`{:08X}", value >> 32, value as u32)
    } else {
        format!("{:08X}", value as u32)
    }
}

pub(super) fn render_scrollbar(
    frame: &mut Frame<'_>,
    body: Rect,
    top: u64,
    length: u64,
    page: u64,
) {
    let track = body.height.saturating_sub(3);
    let scale = |offset: u64| {
        let fraction = u128::from(offset.min(length)) * u128::from(track + 1);
        ((fraction / u128::from(length.max(1))) as u16).saturating_sub(1)
    };
    let start = scale(top);
    let end = if length <= page {
        track
    } else {
        scale(top.saturating_add(page))
    };
    draw_scrollbar(frame, body, start..end);
}

pub(super) fn draw_scrollbar(frame: &mut Frame<'_>, body: Rect, filled: std::ops::Range<u16>) {
    let track = body.height.saturating_sub(3);
    let mut lines = Vec::with_capacity(usize::from(body.height));
    lines.push(Line::raw("↑"));
    lines.extend((0..track).map(|row| Line::raw(if filled.contains(&row) { "▓" } else { "░" })));
    lines.push(Line::raw("↓"));
    lines.push(Line::raw("▼"));
    frame.render_widget(
        Paragraph::new(lines).style(SCROLLBAR),
        Rect::new(body.x, body.y, 1, body.height),
    );
}

// Unicode PC display glyphs, including IBM graphics for the C0 bytes.
const CONTROL_GLYPHS: [char; 32] = [
    ' ', '☺', '☻', '♥', '♦', '♣', '♠', '•', '◘', '○', '◙', '♂', '♀', '♪', '♫', '☼', '►', '◄', '↕',
    '‼', '¶', '§', '▬', '↨', '↑', '↓', '→', '←', '∟', '↔', '▲', '▼',
];
const EXTENDED_GLYPHS: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ',
    'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ',
    'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕',
    '╣', '║', '╗', '╝', '╜', '╛', '┐', '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦',
    '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐',
    '▀', 'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', '≡', '±',
    '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

pub(super) fn pc_glyph(byte: u8) -> char {
    match byte {
        0x00..=0x1F => CONTROL_GLYPHS[usize::from(byte)],
        0x20..=0x7E => char::from(byte),
        0x7F => '⌂',
        _ => EXTENDED_GLYPHS[usize::from(byte - 0x80)],
    }
}
