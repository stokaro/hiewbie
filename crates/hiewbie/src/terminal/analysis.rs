//! Native controls for the shared executable/decoder model.

use hiewbie_core::formats::Endian;
use hiewbie_core::instructions::DecodeMode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::render::{HEADER, popup};
use super::{
    ByteSource, Command, Dialog, KeyCode, KeyEvent, KeyModifiers, PromptKind, Ui, UiAction,
    ViewMode, escaped_text,
};

const PROFILES: [Option<DecodeMode>; 7] = [
    None,
    Some(DecodeMode::X86_16),
    Some(DecodeMode::X86_32),
    Some(DecodeMode::X86_64),
    Some(DecodeMode::Arm),
    Some(DecodeMode::Thumb),
    Some(DecodeMode::Aarch64),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AnalysisPage {
    Overview,
    Regions,
    Fields,
    Slices,
    Profiles,
}

pub(super) struct AnalysisDialog {
    pub page: AnalysisPage,
    pub selected: usize,
}

impl<S: ByteSource> Ui<S> {
    pub(super) fn open_analysis(&mut self, page: AnalysisPage) {
        self.pending_nibble = None;
        self.editor.refresh_analysis();
        self.dialog = Some(Dialog::Analysis(AnalysisDialog { page, selected: 0 }));
    }

    pub(super) fn cycle_bitness(&mut self) -> UiAction {
        if self.editor.mode() != ViewMode::Code {
            self.notice("Bitness selection applies in Code; Ctrl+A selects a decoder");
            return UiAction::Continue;
        }
        self.editor.refresh_analysis();
        let mode = match self.editor.decode_context().mode {
            Some(DecodeMode::X86_16) => DecodeMode::X86_32,
            Some(DecodeMode::X86_32) => DecodeMode::X86_64,
            Some(DecodeMode::X86_64) => DecodeMode::X86_16,
            _ => {
                self.notice("Bitness cycling is x86 16/32/64; use Ctrl+A for ARM/Thumb/AArch64");
                return UiAction::Continue;
            }
        };
        self.notice(format!("Explicit decoder: {mode}"));
        self.dispatch(Command::SetDecodeMode(Some(mode)))
    }

    fn analysis_count(&self, page: AnalysisPage) -> usize {
        if page == AnalysisPage::Profiles {
            return PROFILES.len();
        }
        self.editor.executable().map_or(0, |executable| match page {
            AnalysisPage::Overview | AnalysisPage::Profiles => 0,
            AnalysisPage::Regions => executable.regions.len(),
            AnalysisPage::Fields => executable.fields.len(),
            AnalysisPage::Slices => executable.slices.len(),
        })
    }

    fn analysis_dispatch(&mut self, command: Command, notice: &str) -> bool {
        match self.editor.dispatch(command) {
            Ok(_) => {
                self.notice(notice);
                true
            }
            Err(error) => {
                self.notice(error.to_string());
                false
            }
        }
    }

    pub(super) fn analysis_key(&mut self, mut dialog: AnalysisDialog, key: KeyEvent) {
        self.editor.refresh_analysis();
        if key.modifiers.intersects(
            KeyModifiers::CONTROL
                | KeyModifiers::ALT
                | KeyModifiers::SUPER
                | KeyModifiers::HYPER
                | KeyModifiers::META,
        ) || (!matches!(key.code, KeyCode::Char(_)) && !key.modifiers.is_empty())
        {
            self.notice("Modified key is unavailable in this dialog; Esc closes it");
            self.dialog = Some(Dialog::Analysis(dialog));
            return;
        }
        let count = self.analysis_count(dialog.page);
        dialog.selected = dialog.selected.min(count.saturating_sub(1));
        let mut close = false;
        match key.code {
            KeyCode::Up => dialog.selected = dialog.selected.saturating_sub(1),
            KeyCode::Down => {
                dialog.selected = dialog
                    .selected
                    .saturating_add(1)
                    .min(count.saturating_sub(1))
            }
            KeyCode::PageUp => dialog.selected = dialog.selected.saturating_sub(12),
            KeyCode::PageDown => {
                dialog.selected = dialog
                    .selected
                    .saturating_add(12)
                    .min(count.saturating_sub(1))
            }
            KeyCode::Home => dialog.selected = 0,
            KeyCode::End => dialog.selected = count.saturating_sub(1),
            KeyCode::Char(number @ '0'..='6') if dialog.page == AnalysisPage::Profiles => {
                dialog.selected = (number as u8 - b'0') as usize;
            }
            KeyCode::Char('l' | 'L') if dialog.page == AnalysisPage::Profiles => {
                self.analysis_dispatch(
                    Command::SetEndian(Some(Endian::Little)),
                    "Explicit little-endian decoding",
                );
            }
            KeyCode::Char('b' | 'B') if dialog.page == AnalysisPage::Profiles => {
                self.analysis_dispatch(
                    Command::SetEndian(Some(Endian::Big)),
                    "Big-endian selected; unsupported decoding remains explicit",
                );
            }
            KeyCode::Char('e' | 'E') if dialog.page == AnalysisPage::Profiles => {
                self.analysis_dispatch(
                    Command::SetEndian(None),
                    "Byte order follows the parsed image or raw default",
                );
            }
            KeyCode::Char('r' | 'R') if dialog.page == AnalysisPage::Profiles => {
                let raw = !self.editor.decode_context().raw;
                self.analysis_dispatch(
                    Command::SetRaw(raw),
                    "Raw addressing toggled; decoder selection is independent",
                );
            }
            KeyCode::Char('v' | 'V') if dialog.page == AnalysisPage::Profiles => {
                self.open_prompt(PromptKind::RawBase);
                return;
            }
            KeyCode::F(8) if dialog.page != AnalysisPage::Profiles => {
                dialog.page = AnalysisPage::Overview;
                dialog.selected = 0;
            }
            KeyCode::F(5) if dialog.page != AnalysisPage::Profiles => {
                close = self
                    .analysis_dispatch(Command::GotoEntry, "Moved to the file-backed entry point");
            }
            KeyCode::F(6) if dialog.page != AnalysisPage::Profiles => {
                dialog.page = AnalysisPage::Regions;
                dialog.selected = 0;
            }
            KeyCode::F(4) if dialog.page != AnalysisPage::Profiles => {
                dialog.page = AnalysisPage::Fields;
                dialog.selected = 0;
            }
            KeyCode::Char('s' | 'S') if dialog.page != AnalysisPage::Profiles => {
                dialog.page = AnalysisPage::Slices;
                dialog.selected = 0;
            }
            KeyCode::F(7) | KeyCode::F(9) if dialog.page != AnalysisPage::Profiles => {
                self.notice("Import/export tables are not implemented in this analysis slice");
            }
            KeyCode::Enter => match dialog.page {
                AnalysisPage::Profiles => {
                    if self.analysis_dispatch(
                        Command::SetDecodeMode(PROFILES[dialog.selected]),
                        "Decoder selection applied",
                    ) {
                        close = self.analysis_dispatch(Command::SetMode(ViewMode::Code), "Code view; '/' resyncs, 1..9 follows a direct branch, Backspace returns");
                    }
                }
                AnalysisPage::Regions if count > 0 => {
                    close = self.analysis_dispatch(
                        Command::GotoRegion(dialog.selected),
                        "Moved to the selected region's file bytes",
                    );
                }
                AnalysisPage::Fields if count > 0 => {
                    if let Some(offset) = self
                        .editor
                        .executable()
                        .and_then(|image| image.fields.get(dialog.selected))
                        .map(|field| field.range.offset)
                    {
                        close = self.analysis_dispatch(
                            Command::Goto(offset),
                            "Moved to the header field's exact backing bytes",
                        );
                    }
                }
                AnalysisPage::Slices if count > 0 => {
                    if self.analysis_dispatch(
                        Command::SelectSlice(dialog.selected),
                        "Container slice selected; mapping and decoder metadata refreshed",
                    ) {
                        dialog.page = AnalysisPage::Overview;
                        dialog.selected = 0;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        if !close {
            self.dialog = Some(Dialog::Analysis(dialog));
        }
    }

    pub(super) fn render_analysis_dialog(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        dialog: &AnalysisDialog,
    ) {
        let popup = popup(area, 96, 23);
        let available = popup.height.saturating_sub(7) as usize;
        let title = match dialog.page {
            AnalysisPage::Overview => " Executable headers ",
            AnalysisPage::Regions => " Sections / load regions ",
            AnalysisPage::Fields => " Header fields and backing bytes ",
            AnalysisPage::Slices => " Universal Mach-O slices ",
            AnalysisPage::Profiles => " Decoder and raw addressing ",
        };
        let context = self.editor.decode_context();
        let mode = context
            .mode
            .map_or_else(|| "none".to_owned(), |mode| mode.to_string());
        let source = if context.overridden {
            "override"
        } else {
            "automatic"
        };
        let addressing = if context.raw {
            format!("raw base {:X}", context.base.get())
        } else {
            self.editor
                .executable()
                .and_then(|image| image.image_base)
                .map_or_else(
                    || "mapped image".to_owned(),
                    |base| format!("image base {:X}", base.get()),
                )
        };
        let mut lines = vec![Line::raw(format!(
            "{mode} / {} / {source} / {addressing}",
            context.endian,
        ))];
        if dialog.page == AnalysisPage::Profiles {
            lines.push(Line::raw(
                "Up/Down or 0..6 selects; Enter applies; Esc cancels",
            ));
            let start = dialog.selected.saturating_sub(available.saturating_sub(1));
            for (index, profile) in PROFILES.iter().enumerate().skip(start).take(available) {
                let label = profile.map_or_else(
                    || "Auto from image (raw requires a choice)".to_owned(),
                    |mode| format!("{mode} [{}]", mode.backend()),
                );
                lines.push(selected_line(
                    format!("{index}  {label}"),
                    index == dialog.selected,
                ));
            }
            lines.push(Line::raw(
                "L little / B big / E automatic endian; R raw toggle",
            ));
            lines.push(Line::raw(
                "V sets raw virtual base. Unsupported decoding keeps raw bytes.",
            ));
        } else {
            lines.push(Line::raw(
                "F5 entry / F6 regions / F4 fields / S slices / F8 overview",
            ));
            lines.push(Line::raw(
                "Enter follows selected bytes; F7 imports/F9 exports planned; Esc closes",
            ));
            match self.editor.executable() {
                Some(executable) => {
                    let entry = executable.entry.map_or_else(
                        || "unavailable".to_owned(),
                        |entry| format!("{:016X}", entry.get()),
                    );
                    match dialog.page {
                        AnalysisPage::Overview => {
                            let summary = [
                                format!("Format {} | machine {} | {} bit | {} endian", executable.format, executable.machine, executable.bitness, executable.endian),
                                format!("Entry VA {entry}"),
                                format!("Image base {}", executable.image_base.map_or_else(|| "not declared".to_owned(), |base| format!("{:016X}", base.get()))),
                                format!("Image bytes FO {:016X}, size {:X}", executable.slice.offset.get(), executable.slice.len),
                                format!("Regions {} | fields {} | container slices {}", executable.regions.len(), executable.fields.len(), executable.slices.len()),
                                format!("Selected slice {} | logical generation {}", executable.selected_slice.map_or_else(|| "none".to_owned(), |slice| slice.to_string()), executable.generation),
                                "Headers and mappings only; symbols/imports/relocations remain planned.".to_owned(),
                                "Byte edits do not rebuild headers, checksums, or signatures.".to_owned(),
                            ];
                            lines.extend(summary.into_iter().take(available).map(Line::raw));
                        }
                        page => {
                            let count = self.analysis_count(page);
                            let start = dialog.selected.saturating_sub(available.saturating_sub(1));
                            if count == 0 {
                                lines.push(Line::raw("<no entries available>"));
                            }
                            for index in start..count.min(start.saturating_add(available)) {
                                let text = match page {
                                    AnalysisPage::Regions => {
                                        let region = &executable.regions[index];
                                        format!(
                                            "{index:>3} {} FO {:X}+{:X} VA {:X}+{:X} {}{}{}",
                                            escaped_text(&region.name),
                                            region.file.offset.get(),
                                            region.file.len,
                                            region.address.get(),
                                            region.memory_size,
                                            if region.readable { 'r' } else { '-' },
                                            if region.writable { 'w' } else { '-' },
                                            if region.executable { 'x' } else { '-' }
                                        )
                                    }
                                    AnalysisPage::Fields => {
                                        let field = &executable.fields[index];
                                        format!(
                                            "{} = {:X}  FO {:X}+{:X}",
                                            escaped_text(&field.name),
                                            field.value,
                                            field.range.offset.get(),
                                            field.range.len
                                        )
                                    }
                                    AnalysisPage::Slices => {
                                        let slice = &executable.slices[index];
                                        format!(
                                            "{}: {} CPU {:X}/{:X} FO {:X}+{:X}",
                                            slice.index,
                                            slice.machine,
                                            slice.cpu_type,
                                            slice.cpu_subtype,
                                            slice.file.offset.get(),
                                            slice.file.len
                                        )
                                    }
                                    _ => String::new(),
                                };
                                lines.push(selected_line(text, index == dialog.selected));
                            }
                        }
                    }
                }
                None => {
                    lines.push(Line::raw("<raw bytes; executable metadata is unavailable>"));
                    if let Some(problem) = self.editor.analysis_problem() {
                        lines.push(Line::raw(escaped_text(problem)));
                    }
                    lines.push(Line::raw(
                        "Ctrl+A selects an explicit decoder and raw address base.",
                    ));
                }
            }
        }
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(lines)
                .style(HEADER)
                .block(Block::default().borders(Borders::ALL).title(title)),
            popup,
        );
    }
}

fn selected_line(text: String, selected: bool) -> Line<'static> {
    Line::styled(
        text,
        if selected {
            Style::new().fg(Color::Black).bg(Color::Yellow)
        } else {
            HEADER
        },
    )
}

pub(super) fn parse_base(input: &str) -> Result<u64, String> {
    hiewbie_core::parse_virtual_address(input)
        .map(|address| address.get())
        .map_err(|error| error.to_string())
}
