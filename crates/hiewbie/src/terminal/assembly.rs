use hiewbie_core::assembly::{AssemblyError, MAX_SOURCE_BYTES};
use hiewbie_core::formats::VirtualAddress;
use hiewbie_core::instructions::{DecodeMode, DecodeStatus, DecodedInstruction};
use hiewbie_core::{
    AssemblyPadding, AssemblyPatch, AssemblySpan, ByteSource, Editor, Error, MAX_EDIT_BYTES,
};
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Color;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use super::render::{
    ACTIVE_SEARCH_FIELD, ERROR_PANEL, FOCUSED_ROW, HEADER, PANEL, dialog_surface,
    dialog_surface_with_bottom_title, row,
};
use super::{
    Dialog, Help, HelpTopic, KeyCode, KeyEvent, KeyModifiers, Ui, escaped_text, is_control_char,
};

pub(super) struct AssemblyInput {
    source: String,
    caret: usize,
    mode: DecodeMode,
    address: VirtualAddress,
    length: usize,
    stage: Stage,
    error: Option<String>,
    pub(super) help: Option<Help>,
}

enum Stage {
    Input,
    Choice(Choices),
    Review(Review),
}

struct Choices {
    bytes: Vec<Vec<u8>>,
    selected: usize,
}

struct Review {
    candidate: usize,
    bytes: Vec<u8>,
    span: usize,
    padding: AssemblyPadding,
    patch: Result<AssemblyPatch, Error>,
    first_byte: usize,
}

impl AssemblyInput {
    pub(super) fn at<S: ByteSource>(editor: &mut Editor<S>) -> Result<Self, Error> {
        let (mode, instruction) = editor.assembly_site()?;
        let length = instruction.length;
        let address = instruction.address;
        let source = seed_source(mode, instruction);
        Ok(Self {
            caret: source.len(),
            source,
            mode,
            address,
            length,
            stage: Stage::Input,
            error: None,
            help: None,
        })
    }

    pub(super) const fn has_caret(&self) -> bool {
        self.error.is_none() && self.help.is_none() && matches!(self.stage, Stage::Input)
    }

    pub(super) fn function_labels(&self) -> [&'static str; 12] {
        let mut labels = [""; 12];
        if self.error.is_none() && matches!(self.stage, Stage::Input) {
            labels[0] = "Help";
            labels[3] = "Select";
        }
        labels
    }

    fn input_key(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Left, KeyModifiers::NONE) => self.caret = self.caret.saturating_sub(1),
            (KeyCode::Right, KeyModifiers::NONE) => {
                self.caret = (self.caret + 1).min(self.source.len());
            }
            (KeyCode::Home, KeyModifiers::NONE) => self.caret = 0,
            (KeyCode::End, KeyModifiers::NONE) => self.caret = self.source.len(),
            (KeyCode::Backspace, KeyModifiers::NONE) if self.caret > 0 => {
                self.caret -= 1;
                self.source.remove(self.caret);
            }
            (KeyCode::Delete, KeyModifiers::NONE) if self.caret < self.source.len() => {
                self.source.remove(self.caret);
            }
            _ if is_control_char(key, 'u') => {
                self.source.clear();
                self.caret = 0;
            }
            (KeyCode::Char(character), KeyModifiers::NONE | KeyModifiers::SHIFT)
                if character.is_ascii() && !character.is_ascii_control() =>
            {
                if self.source.len() < MAX_SOURCE_BYTES {
                    self.source.insert(self.caret, character);
                    self.caret += 1;
                } else {
                    self.reject(Error::Assembly(AssemblyError::SourceLimit));
                }
            }
            _ => {}
        }
    }

    fn reject(&mut self, error: Error) {
        self.error = Some(match error {
            Error::Assembly(AssemblyError::UnknownMnemonic(_)) => "Illegal instruction".into(),
            Error::Assembly(AssemblyError::UnresolvedSymbol(_)) => "Name not found".into(),
            other => escaped_text(&other.to_string()),
        });
    }
}

fn seed_source(mode: DecodeMode, instruction: DecodedInstruction) -> String {
    if instruction.status != DecodeStatus::Decoded || !instruction.text.is_ascii() {
        return String::new();
    }
    if mode.is_x86()
        && let Some((mnemonic @ ("jmp" | "call"), operand)) = instruction.text.split_once(" far ")
    {
        for (intel, hiew) in [
            ("dword ptr ", "d"),
            ("fword ptr ", "f"),
            ("tbyte ptr ", "t"),
        ] {
            if let Some(address) = operand.strip_prefix(intel) {
                let mnemonic = if mnemonic == "jmp" { "jmpf" } else { "callf" };
                return format!("{mnemonic:<13}{hiew},{address}");
            }
        }
    }
    if mode.is_x86()
        && let Some(target) = instruction.direct_branch
        && instruction.text.starts_with("jmp ")
    {
        let short = instruction.bytes().iter().rev().nth(1) == Some(&0xeb);
        let mnemonic = if short { "jmps" } else { "jmp" };
        return format!("{mnemonic} {:X}", target.address.get());
    }
    instruction.text
}

impl<S: ByteSource> Ui<S> {
    pub(super) fn assembly_key(&mut self, mut input: Box<AssemblyInput>, key: KeyEvent) {
        if let Some(help) = &mut input.help {
            if help.closes(key) {
                input.help = None;
            }
            self.dialog = Some(Dialog::Assembly(input));
            return;
        }
        let cancel = key.code == KeyCode::Esc || is_control_char(key, 'c');
        let enter = key.code == KeyCode::Enter && key.modifiers.is_empty();
        let inspect = is_control_char(key, 'p');
        let mut commit = None;
        if input.error.is_some() {
            if cancel || enter {
                input.error = None;
            }
        } else {
            match &mut input.stage {
                Stage::Input if cancel => return,
                Stage::Input
                    if (key.code, key.modifiers) == (KeyCode::F(1), KeyModifiers::NONE) =>
                {
                    input.help = Some(Help::new(HelpTopic::Assembly));
                }
                Stage::Input
                    if (key.code, key.modifiers) == (KeyCode::F(4), KeyModifiers::NONE) =>
                {
                    self.assembly_select = !self.assembly_select;
                }
                Stage::Input if enter && !self.assembly_select => {
                    commit = Some(self.editor.preview_assembly_patch(
                        &input.source,
                        0,
                        AssemblySpan::CurrentInstruction,
                        AssemblyPadding::Preserve,
                    ));
                }
                Stage::Input if enter || inspect => {
                    match self.editor.assemble_at_cursor(&input.source) {
                        Ok(bytes) if self.assembly_select && bytes.len() > 1 => {
                            input.stage = Stage::Choice(Choices { bytes, selected: 0 });
                        }
                        Ok(mut bytes) if inspect => {
                            self.review_assembly(&mut input, 0, bytes.remove(0))
                        }
                        Ok(_) => {
                            commit = Some(self.editor.preview_assembly_patch(
                                &input.source,
                                0,
                                AssemblySpan::CurrentInstruction,
                                AssemblyPadding::Preserve,
                            ))
                        }
                        Err(error) => input.reject(error),
                    }
                }
                Stage::Input => input.input_key(key),
                Stage::Choice(_) if cancel => {
                    input.stage = Stage::Input;
                    input.error = Some("Canceled".into());
                }
                Stage::Choice(choices) if enter => {
                    commit = Some(self.editor.preview_assembly_patch(
                        &input.source,
                        choices.selected,
                        AssemblySpan::CurrentInstruction,
                        AssemblyPadding::Preserve,
                    ));
                }
                Stage::Choice(choices) if inspect => {
                    let candidate = choices.selected;
                    let bytes = choices.bytes[candidate].clone();
                    self.review_assembly(&mut input, candidate, bytes);
                }
                Stage::Choice(choices) if key.modifiers.is_empty() => {
                    let last = choices.bytes.len() - 1;
                    choices.selected = match key.code {
                        KeyCode::Up => choices.selected.saturating_sub(1),
                        KeyCode::Down => (choices.selected + 1).min(last),
                        KeyCode::Home => 0,
                        KeyCode::End => last,
                        _ => choices.selected,
                    };
                }
                Stage::Review(_) if cancel => input.stage = Stage::Input,
                Stage::Review(review) if enter && review.patch.is_ok() => {
                    let Stage::Review(review) = std::mem::replace(&mut input.stage, Stage::Input)
                    else {
                        unreachable!();
                    };
                    commit = Some(review.patch);
                }
                Stage::Review(review)
                    if matches!(key.modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT) =>
                {
                    let step = if input.mode == DecodeMode::Aarch64 {
                        4
                    } else {
                        1
                    };
                    let span = review.span;
                    let padding = review.padding;
                    match key.code {
                        KeyCode::Char('w' | 'W') => review.span = span.max(review.bytes.len()),
                        KeyCode::Char('+') => {
                            review.span = span.saturating_add(step).min(MAX_EDIT_BYTES)
                        }
                        KeyCode::Char('-') => {
                            review.span = span.saturating_sub(step).max(input.length)
                        }
                        KeyCode::Char('p' | 'P') => {
                            review.padding = if padding == AssemblyPadding::Nop {
                                AssemblyPadding::Preserve
                            } else {
                                AssemblyPadding::Nop
                            };
                        }
                        KeyCode::Left => review.first_byte = review.first_byte.saturating_sub(1),
                        KeyCode::Right => review.first_byte = (review.first_byte + 1).min(span - 1),
                        _ => {}
                    }
                    if span != review.span || padding != review.padding {
                        review.first_byte = review.first_byte.min(review.span - 1);
                        review.patch = self.editor.preview_assembly_patch(
                            &input.source,
                            review.candidate,
                            AssemblySpan::Explicit(review.span),
                            review.padding,
                        );
                    }
                }
                Stage::Choice(_) | Stage::Review(_) => {}
            }
        }
        if let Some(patch) = commit {
            match patch.and_then(|patch| {
                self.editor.apply_assembly_patch(&patch)?;
                Ok(patch.range().offset.get() + patch.replacement().len() as u64)
            }) {
                Ok(next_offset) => {
                    self.pending_nibble = None;
                    if next_offset < self.editor.document().len()
                        && let Ok(next) = AssemblyInput::at(&mut self.editor)
                    {
                        self.dialog = Some(Dialog::Assembly(Box::new(next)));
                    }
                    return;
                }
                Err(error) => input.reject(error),
            }
        }
        self.dialog = Some(Dialog::Assembly(input));
    }

    fn review_assembly(&mut self, input: &mut AssemblyInput, candidate: usize, bytes: Vec<u8>) {
        let patch = self.editor.preview_assembly_patch(
            &input.source,
            candidate,
            AssemblySpan::CurrentInstruction,
            AssemblyPadding::Preserve,
        );
        input.stage = Stage::Review(Review {
            candidate,
            bytes,
            span: input.length,
            padding: AssemblyPadding::Preserve,
            patch,
            first_byte: 0,
        });
    }
}

pub(super) fn render_assembly(
    frame: &mut Frame<'_>,
    area: Rect,
    input: &AssemblyInput,
    select: bool,
) {
    let area = Rect::new(
        area.x,
        area.y.saturating_add(1),
        area.width,
        area.height.saturating_sub(1),
    );
    if let Some(error) = &input.error {
        let content = dialog_surface(frame, area, 48, 5, "", ERROR_PANEL);
        if content.height > 0 {
            let width = Line::from(error.as_str()).width();
            let pad = usize::from(content.width).saturating_sub(width) / 2;
            frame.render_widget(
                Paragraph::new(format!("{:pad$}{error}", "")).style(ERROR_PANEL),
                content,
            );
        }
        return;
    }
    match &input.stage {
        Stage::Input => render_input(frame, area, input, select),
        Stage::Choice(choices) => render_choices(frame, area, choices),
        Stage::Review(review) => render_review(frame, area, input, review),
    }
}

fn render_input(frame: &mut Frame<'_>, area: Rect, input: &AssemblyInput, select: bool) {
    let label = if select {
        " CommandSelect: On  "
    } else {
        " CommandSelect: Off "
    };
    let content =
        dialog_surface_with_bottom_title(frame, area, 72, 5, Line::from(label).centered(), PANEL);
    let field = content.inner(Margin::new(1, 0));
    if field.width == 0 || field.height == 0 {
        return;
    }
    let width = usize::from(field.width);
    let first = input.caret.saturating_sub(width - 1);
    let shown = &input.source[first..(first + width).min(input.source.len())];
    let line = format!("{shown}{}", "░".repeat(width - shown.len()));
    frame.render_widget(
        Paragraph::new(line).style(ACTIVE_SEARCH_FIELD),
        row(field, 0),
    );
    frame.set_cursor_position((field.x + (input.caret - first) as u16, field.y));
}

fn render_choices(frame: &mut Frame<'_>, area: Rect, choices: &Choices) {
    let content = dialog_surface(
        frame,
        area,
        38,
        choices.bytes.len() as u16 + 4,
        Line::from(" Select command ").centered(),
        HEADER,
    )
    .inner(Margin::new(1, 0));
    let visible = usize::from(content.height);
    let first = choices.selected.saturating_sub(visible.saturating_sub(1));
    for (index, bytes) in choices.bytes.iter().enumerate().skip(first).take(visible) {
        let style = if index == choices.selected {
            FOCUSED_ROW
        } else {
            HEADER.fg(Color::Blue)
        };
        frame.render_widget(
            Paragraph::new(hex(bytes.iter(), "")).style(style),
            row(content, (index - first) as u16),
        );
    }
}

fn render_review(frame: &mut Frame<'_>, area: Rect, input: &AssemblyInput, review: &Review) {
    let content = dialog_surface(
        frame,
        area,
        72,
        10,
        Line::from(" Patch preview ").centered(),
        PANEL,
    );
    let first = review.first_byte;
    let count = usize::from(content.width.saturating_sub(8) / 3);
    let padding = if review.padding == AssemblyPadding::Nop {
        "NOP"
    } else {
        "preserve"
    };
    let line = format!(
        "{}  {:X}  {} byte(s)  {}",
        input.mode,
        input.address.get(),
        review.span,
        padding
    );
    let mut lines = vec![line];
    match &review.patch {
        Ok(patch) => {
            lines.push(format!("From +{:X}", first));
            lines.push(format!(
                "Before  {}",
                hex(patch.original().iter().skip(first).take(count), " ")
            ));
            let after = patch
                .replacement()
                .iter()
                .chain(&patch.original()[patch.replacement().len()..]);
            lines.push(format!(
                "After   {}",
                hex(after.skip(first).take(count), " ")
            ));
        }
        Err(error) => {
            lines.push(format!(
                "Code    {}",
                hex(review.bytes.iter().take(count), " ")
            ));
            lines.push(escaped_text(&error.to_string()));
        }
    }
    lines.push("W widen to fit  +/- span  P pad  Arrows scroll".into());
    lines.push("Enter apply  Esc cancel".into());
    for (index, line) in lines
        .into_iter()
        .take(usize::from(content.height))
        .enumerate()
    {
        frame.render_widget(
            Paragraph::new(line).style(PANEL),
            row(content, index as u16),
        );
    }
}

fn hex<'a>(bytes: impl Iterator<Item = &'a u8>, separator: &str) -> String {
    bytes
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(separator)
}
