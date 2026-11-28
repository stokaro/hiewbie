use crate::formats::{Endian, VirtualAddress};
use crate::instructions::DecodeMode;
use crate::{
    AssemblyPadding, AssemblySpan, Command, Document, Editor, Error, FileOffset, MemorySource,
    ViewMode,
};

use crate::aarch64_images;

fn raw(bytes: &[u8], mode: DecodeMode, read_only: bool) -> Editor<MemorySource> {
    let mut editor = Editor::new(Document::new(MemorySource::new(bytes.to_vec())), read_only);
    editor.dispatch(Command::SetRaw(true)).unwrap();
    editor
        .dispatch(Command::SetRawBase(VirtualAddress::new(0x1000)))
        .unwrap();
    editor.dispatch(Command::SetDecodeMode(Some(mode))).unwrap();
    editor.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
    editor
}

fn logical(editor: &mut Editor<MemorySource>) -> Vec<u8> {
    let mut bytes = vec![0; editor.document().len() as usize];
    let count = editor
        .document_mut()
        .read_at(FileOffset::new(0), &mut bytes)
        .unwrap();
    assert_eq!(count, bytes.len());
    bytes
}

#[test]
fn x86_short_patch_writes_only_its_prefix_and_has_one_undo_boundary() {
    let original = [0xb8, 1, 0, 0, 0, 0xcc];
    let mut editor = raw(&original, DecodeMode::X86_32, false);
    editor.dispatch(Command::BeginEdit).unwrap();
    let patch = editor
        .preview_assembly_patch(
            "nop",
            0,
            AssemblySpan::CurrentInstruction,
            AssemblyPadding::Preserve,
        )
        .unwrap();
    assert_eq!(patch.address(), VirtualAddress::new(0x1000));
    assert_eq!(patch.range().offset, FileOffset::new(0));
    assert_eq!(patch.range().len, 5);
    assert_eq!(patch.original(), &original[..5]);
    assert_eq!(patch.replacement(), [0x90]);
    assert_eq!(logical(&mut editor), original);
    assert_eq!(editor.document().generation(), 0);

    editor.apply_assembly_patch(&patch).unwrap();
    let replaced = [0x90, 1, 0, 0, 0, 0xcc];
    assert_eq!(logical(&mut editor), replaced);
    assert_eq!(editor.cursor(), FileOffset::new(1));
    editor.dispatch(Command::Undo).unwrap();
    assert_eq!(logical(&mut editor), original);
    assert!(!editor.document_mut().undo().unwrap());
    editor.dispatch(Command::Redo).unwrap();
    assert_eq!(logical(&mut editor), replaced);
    editor.dispatch(Command::DiscardEdit).unwrap();
    assert_eq!(logical(&mut editor), original);
    assert!(!editor.document().is_dirty());
}

#[test]
fn x86_widening_and_padding_are_explicit_and_preserve_neighboring_bytes() {
    let original = [0x90, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc];
    let mut editor = raw(&original, DecodeMode::X86_32, false);
    editor.dispatch(Command::BeginEdit).unwrap();
    for (span, candidate) in [
        (AssemblySpan::CurrentInstruction, 0),
        (AssemblySpan::Explicit(0), 0),
        (AssemblySpan::Explicit(4), 0),
        (AssemblySpan::Explicit(8), 0),
        (AssemblySpan::Explicit(6), 99),
    ] {
        assert!(
            editor
                .preview_assembly_patch("jmp 1010", candidate, span, AssemblyPadding::Nop)
                .is_err()
        );
    }
    let patch = editor
        .preview_assembly_patch(
            "jmp 1010",
            0,
            AssemblySpan::Explicit(6),
            AssemblyPadding::Nop,
        )
        .unwrap();
    assert_eq!(patch.original(), &original[..6]);
    assert_eq!(patch.replacement(), [0xe9, 0x0b, 0, 0, 0, 0x90]);
    assert_eq!(logical(&mut editor), original);
    editor.apply_assembly_patch(&patch).unwrap();
    assert_eq!(editor.cursor(), FileOffset::new(6));
    assert_eq!(logical(&mut editor), [0xe9, 0x0b, 0, 0, 0, 0x90, 0xcc]);
    editor.dispatch(Command::Undo).unwrap();
    assert_eq!(logical(&mut editor), original);
    assert!(editor.document_mut().redo().unwrap());
}

#[test]
fn arm64_patch_uses_fixed_width_nops_and_refuses_an_unmapped_span() {
    use aarch64_images::{ARM64_BASE, ARM64_CODE_OFFSET, arm64_elf};

    let mut original = arm64_elf(true);
    original.extend_from_slice(&[0x55; 8]);
    let mut editor = Editor::new(Document::new(MemorySource::new(original.clone())), false);
    editor.dispatch(Command::GotoEntry).unwrap();
    editor.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
    editor.dispatch(Command::BeginEdit).unwrap();
    let branch = format!("b {:x}", ARM64_BASE + 0x18);
    assert!(
        editor
            .preview_assembly_patch(&branch, 0, AssemblySpan::Explicit(7), AssemblyPadding::Nop)
            .is_err()
    );
    let patch = editor
        .preview_assembly_patch(&branch, 0, AssemblySpan::Explicit(8), AssemblyPadding::Nop)
        .unwrap();
    assert_eq!(patch.replacement(), [6, 0, 0, 0x14, 0x1f, 0x20, 3, 0xd5]);
    editor.apply_assembly_patch(&patch).unwrap();
    let start = ARM64_CODE_OFFSET as usize;
    let logical_bytes = logical(&mut editor);
    assert_eq!(&logical_bytes[start..start + 8], patch.replacement());
    assert_eq!(&logical_bytes[start + 8..], &original[start + 8..]);
    assert_eq!(editor.cursor().get(), ARM64_CODE_OFFSET + 8);
    editor.dispatch(Command::Undo).unwrap();
    assert_eq!(logical(&mut editor), original);
    editor
        .dispatch(Command::Goto(FileOffset::new(ARM64_CODE_OFFSET + 0x3c)))
        .unwrap();
    assert!(
        editor
            .preview_assembly_patch("nop", 0, AssemblySpan::Explicit(8), AssemblyPadding::Nop)
            .is_err()
    );
    assert!(editor.document_mut().redo().unwrap());
}

#[test]
fn review_is_bound_to_the_edit_session_cursor_bytes_and_profile() {
    let original = [0xcc, 0xcc, 0xcc];
    let mut editor = raw(&original, DecodeMode::X86_32, false);
    let mut other = raw(&original, DecodeMode::X86_32, false);
    editor.dispatch(Command::BeginEdit).unwrap();
    other.dispatch(Command::BeginEdit).unwrap();
    let patch = editor
        .preview_assembly_patch(
            "nop",
            0,
            AssemblySpan::CurrentInstruction,
            AssemblyPadding::Preserve,
        )
        .unwrap();
    assert!(other.apply_assembly_patch(&patch).is_err());
    assert_eq!(logical(&mut other), original);
    editor.dispatch(Command::Move(1)).unwrap();
    assert!(editor.apply_assembly_patch(&patch).is_err());
    editor.dispatch(Command::Move(-1)).unwrap();
    editor
        .dispatch(Command::SetEndian(Some(Endian::Big)))
        .unwrap();
    assert!(editor.apply_assembly_patch(&patch).is_err());
    editor.dispatch(Command::SetEndian(None)).unwrap();

    editor.dispatch(Command::DiscardEdit).unwrap();
    assert!(matches!(
        editor.apply_assembly_patch(&patch),
        Err(Error::NotEditing)
    ));
    editor.dispatch(Command::BeginEdit).unwrap();
    assert!(editor.apply_assembly_patch(&patch).is_err());
    let patch = editor
        .preview_assembly_patch(
            "nop",
            0,
            AssemblySpan::CurrentInstruction,
            AssemblyPadding::Preserve,
        )
        .unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(2), &[2])
        .unwrap();
    editor.dispatch(Command::Undo).unwrap();
    assert!(editor.apply_assembly_patch(&patch).is_err());
    assert_eq!(logical(&mut editor), original);
    assert!(editor.document_mut().redo().unwrap());

    let mut read_only = raw(&original, DecodeMode::X86_32, true);
    assert!(matches!(
        read_only.preview_assembly_patch(
            "nop",
            0,
            AssemblySpan::CurrentInstruction,
            AssemblyPadding::Preserve,
        ),
        Err(Error::ReadOnly)
    ));
    assert!(matches!(
        read_only.apply_assembly_patch(&patch),
        Err(Error::ReadOnly)
    ));
}

#[test]
fn encoding_selection_previews_each_candidate_before_commit() {
    let original = [0xcc; 6];
    let mut editor = raw(&original, DecodeMode::X86_32, false);
    editor.dispatch(Command::BeginEdit).unwrap();
    let first = editor
        .preview_assembly_patch(
            "nop",
            0,
            AssemblySpan::Explicit(3),
            AssemblyPadding::Preserve,
        )
        .unwrap();
    let second = editor
        .preview_assembly_patch(
            "nop",
            2,
            AssemblySpan::Explicit(3),
            AssemblyPadding::Preserve,
        )
        .unwrap();
    assert_eq!(first.replacement(), [0x90]);
    assert_eq!(second.replacement(), [0x0f, 0x1f, 0x00]);
    assert_eq!(logical(&mut editor), original);
    assert_eq!(editor.cursor(), FileOffset::new(0));
    editor.apply_assembly_patch(&second).unwrap();
    assert_eq!(logical(&mut editor), [0x0f, 0x1f, 0x00, 0xcc, 0xcc, 0xcc]);
    assert!(editor.apply_assembly_patch(&first).is_err());
    editor.dispatch(Command::Undo).unwrap();
    assert_eq!(logical(&mut editor), original);
}

#[test]
fn selected_span_cannot_hide_an_overlapping_mapping() {
    use aarch64_images::{ARM64_BASE, ARM64_CODE_OFFSET, arm64_elf};

    let mut original = arm64_elf(false);
    original[56..58].copy_from_slice(&2_u16.to_le_bytes());
    // A second PT_LOAD maps four interior bytes to another virtual address.
    original[120..124].copy_from_slice(&1_u32.to_le_bytes());
    for (at, word) in [
        (128, ARM64_CODE_OFFSET + 8),
        (136, ARM64_BASE + 0x1008),
        (152, 4),
        (160, 4),
        (168, 4),
    ] {
        original[at..at + 8].copy_from_slice(&word.to_le_bytes());
    }
    let mut editor = Editor::new(Document::new(MemorySource::new(original.clone())), false);
    editor.dispatch(Command::GotoEntry).unwrap();
    editor.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
    editor.dispatch(Command::BeginEdit).unwrap();
    assert!(
        editor
            .preview_assembly_patch("nop", 0, AssemblySpan::Explicit(16), AssemblyPadding::Nop)
            .is_err()
    );
    assert_eq!(logical(&mut editor), original);
    assert!(!editor.document_mut().undo().unwrap());
}
