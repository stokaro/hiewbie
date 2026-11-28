//! Independently generated images: exact bytes, no external executable fixtures.

use crate::formats::{Endian, Machine, VirtualAddress};
use crate::instructions::{DecodeMode, DecodeStatus};
use crate::*;

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn pe32() -> Vec<u8> {
    let mut bytes = vec![0; 0x400];
    bytes[..2].copy_from_slice(b"MZ");
    put32(&mut bytes, 0x3c, 0x80);
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    put16(&mut bytes, 0x84, 0x14c);
    put16(&mut bytes, 0x86, 1);
    put16(&mut bytes, 0x94, 224);
    put16(&mut bytes, 0x96, 0x102);
    let optional = 0x98;
    put16(&mut bytes, optional, 0x10b);
    put32(&mut bytes, optional + 16, 0x1000);
    put32(&mut bytes, optional + 28, 0x400000);
    put32(&mut bytes, optional + 32, 0x1000);
    put32(&mut bytes, optional + 36, 0x200);
    put32(&mut bytes, optional + 56, 0x2000);
    put32(&mut bytes, optional + 60, 0x200);
    let section = optional + 224;
    bytes[section..section + 5].copy_from_slice(b".text");
    put32(&mut bytes, section + 8, 0x100);
    put32(&mut bytes, section + 12, 0x1000);
    put32(&mut bytes, section + 16, 0x200);
    put32(&mut bytes, section + 20, 0x200);
    put32(&mut bytes, section + 36, 0x60000020);
    bytes[0x200..0x208].copy_from_slice(&[0xeb, 4, 0x90, 0x90, 0x90, 0x90, 0xc3, 0x90]);
    bytes
}

fn arm64_macho() -> Vec<u8> {
    let mut bytes = vec![0; 0x210];
    put32(&mut bytes, 0, 0xfeedfacf);
    put32(&mut bytes, 4, 0x0100000c);
    put32(&mut bytes, 12, 2);
    put32(&mut bytes, 16, 2);
    put32(&mut bytes, 20, 96);
    put32(&mut bytes, 32, 0x19);
    put32(&mut bytes, 36, 72);
    bytes[40..46].copy_from_slice(b"__TEXT");
    put64(&mut bytes, 56, 0x100000000);
    put64(&mut bytes, 64, 0x1000);
    put64(&mut bytes, 80, 0x210);
    put32(&mut bytes, 88, 7);
    put32(&mut bytes, 92, 5);
    put32(&mut bytes, 104, 0x80000028);
    put32(&mut bytes, 108, 24);
    put64(&mut bytes, 112, 0x200);
    bytes[0x200..0x20c].copy_from_slice(&[2, 0, 0, 0x14, 0x1f, 0x20, 3, 0xd5, 0xc0, 3, 0x5f, 0xd6]);
    bytes
}

fn editor(bytes: Vec<u8>) -> Editor<MemorySource> {
    Editor::new(Document::new(MemorySource::new(bytes)), false)
}

fn raw(bytes: Vec<u8>, mode: DecodeMode) -> Editor<MemorySource> {
    let mut editor = editor(bytes);
    editor.dispatch(Command::SetRaw(true)).unwrap();
    editor.dispatch(Command::SetDecodeMode(Some(mode))).unwrap();
    editor
}

#[test]
fn identical_pe_and_arm64_macho_workflows_are_independent_of_host_cpu() {
    for (bytes, mode, entry, destination) in [
        (pe32(), DecodeMode::X86_32, 0x401000, 0x206),
        (arm64_macho(), DecodeMode::Aarch64, 0x100000200, 0x208),
    ] {
        let mut editor = editor(bytes);
        editor.refresh_analysis();
        assert_eq!(editor.decode_context().mode, Some(mode));
        assert!(!editor.decode_context().overridden);
        editor.dispatch(Command::GotoEntry).unwrap();
        assert_eq!(editor.cursor().get(), 0x200);
        let rows = editor.code_rows(4).unwrap();
        assert_eq!(rows[0].instruction.address.get(), entry);
        assert_eq!(rows[0].instruction.status, DecodeStatus::Decoded);
        assert_eq!(rows[0].branch_number, Some(1));
        editor.dispatch(Command::FollowBranch(1)).unwrap();
        assert_eq!(editor.cursor().get(), destination);
        editor.dispatch(Command::ReturnBranch).unwrap();
        assert_eq!(editor.cursor().get(), 0x200);
        assert!(!editor.decode_context().overridden);
    }
}

fn complete_reference_search(editor: &mut Editor<MemorySource>, budget: usize) -> SearchProgress {
    for _ in 0..editor.document().len().max(1) {
        match editor.step_search(budget).unwrap().unwrap() {
            SearchProgress::Pending { .. } => {}
            result => return result,
        }
    }
    panic!("reference search did not advance");
}

#[test]
fn hiew_16_branch_navigation_and_aligned_search_share_instruction_boundaries() {
    // HIEW sees C5, F8, then JA at byte 2. The portable decoder reads
    // C5F877 as one VEX instruction and skips the overlapping branch.
    let data = vec![0xc5, 0xf8, 0x77, 1, 0x90, 0xc3];
    let target = FileOffset::new(5);
    let mut portable = raw(data.clone(), DecodeMode::X86_16);
    portable.dispatch(Command::Goto(target)).unwrap();
    portable.dispatch(Command::FindReferences).unwrap();
    assert_eq!(
        complete_reference_search(&mut portable, 1),
        SearchProgress::NotFound
    );

    let mut hiew = raw(data, DecodeMode::X86_16).with_hiew_x86_decode();
    hiew.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
    let rows = hiew.code_viewport(FileOffset::new(0), 5).unwrap();
    assert_eq!(rows[0].instruction.bytes(), &[0xc5]);
    assert_eq!(rows[0].instruction.status, DecodeStatus::InvalidEncoding);
    assert_eq!(rows[2].offset, FileOffset::new(2));
    assert_eq!(rows[2].branch_number, Some(1));
    hiew.dispatch(Command::MoveCode(1)).unwrap();
    assert_eq!(hiew.cursor(), FileOffset::new(1));
    hiew.dispatch(Command::MoveCode(1)).unwrap();
    assert_eq!(hiew.cursor(), FileOffset::new(2));
    hiew.dispatch(Command::FollowBranch(1)).unwrap();
    assert_eq!(hiew.cursor(), target);
    hiew.dispatch(Command::FindReferences).unwrap();
    assert_eq!(
        complete_reference_search(&mut hiew, 1),
        SearchProgress::Found(FileOffset::new(2))
    );
    assert!(!hiew.document().is_dirty());
}

#[test]
fn raw_reference_search_follows_instructions_and_preserves_return_history() {
    let mut bytes = vec![0xcc; 0x80];
    bytes[..2].copy_from_slice(&[0xeb, 0x3e]);
    bytes[8..13].copy_from_slice(&[0xe8, 0x33, 0, 0, 0]);
    bytes[0x10..0x15].copy_from_slice(&[0xa1, 0x40, 0, 0, 0]);
    bytes[0x50..0x52].copy_from_slice(&[0xeb, 0xee]);
    // A branch encoding inside MOV's immediate, visible only in bytewise scanning.
    bytes[0x70..0x75].copy_from_slice(&[0xb8, 0xeb, 0xcd, 0, 0]);
    let mut editor = editor(bytes);
    editor
        .dispatch(Command::Goto(FileOffset::new(0x40)))
        .unwrap();
    editor.dispatch(Command::FindReferences).unwrap();
    for expected in [0, 8, 0x50] {
        let result = complete_reference_search(&mut editor, 1);
        assert_eq!(result, SearchProgress::Found(FileOffset::new(expected)));
        assert_eq!(editor.mode(), ViewMode::Code);
        assert_eq!(editor.cursor().get(), expected);
        editor.dispatch(Command::NextReference).unwrap();
    }
    assert_eq!(
        complete_reference_search(&mut editor, 1),
        SearchProgress::NotFound
    );
    assert_eq!(editor.cursor().get(), 0x50);
    for expected in [0x50, 8, 0, 0x50] {
        editor.dispatch(Command::ReturnBranch).unwrap();
        assert_eq!(editor.cursor().get(), expected);
    }
    editor
        .dispatch(Command::Goto(FileOffset::new(0x40)))
        .unwrap();
    editor.dispatch(Command::ToggleReferenceAlignment).unwrap();
    editor.dispatch(Command::FindReferences).unwrap();
    for expected in [0, 8, 0x50, 0x71] {
        assert_eq!(
            complete_reference_search(&mut editor, 1),
            SearchProgress::Found(FileOffset::new(expected))
        );
        editor.dispatch(Command::NextReference).unwrap();
    }
    assert_eq!(
        complete_reference_search(&mut editor, 1),
        SearchProgress::NotFound
    );
    assert!(!editor.document().is_dirty());
}

#[test]
fn hiew_wide_aligned_scan_finds_the_branch_after_invalid_vex_l() {
    for mode in [DecodeMode::X86_32, DecodeMode::X86_64] {
        // The portable VPXOR consumes EB as ModR/M. HIEW rejects its
        // 256-bit form and sees a short jump beginning with EB.
        let data = vec![0xc5, 0xed, 0xef, 0xeb, 1, 0x90, 0xc3];
        let target = FileOffset::new(6);
        let mut portable = raw(data.clone(), mode);
        portable.dispatch(Command::Goto(target)).unwrap();
        portable.dispatch(Command::FindReferences).unwrap();
        assert_eq!(
            complete_reference_search(&mut portable, 1),
            SearchProgress::NotFound
        );

        let mut hiew = raw(data, mode).with_hiew_x86_decode();
        hiew.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
        let rows = hiew.code_viewport(FileOffset::new(0), 5).unwrap();
        assert_eq!(rows[0].instruction.text, "#UD(vex.L)");
        assert_eq!(rows[1].offset, FileOffset::new(3));
        assert_eq!(rows[1].branch_number, Some(1));
        hiew.dispatch(Command::MoveCode(1)).unwrap();
        assert_eq!(hiew.cursor(), FileOffset::new(3));
        hiew.dispatch(Command::FollowBranch(1)).unwrap();
        assert_eq!(hiew.cursor(), target);
        hiew.dispatch(Command::FindReferences).unwrap();
        assert_eq!(
            complete_reference_search(&mut hiew, 1),
            SearchProgress::Found(FileOffset::new(3)),
        );
        assert!(!hiew.document().is_dirty());
    }
}

#[test]
fn mapped_reference_search_matches_virtual_operands_and_rejects_stale_continuation() {
    let mut bytes = pe32();
    let optional = 0x98;
    let section = optional + 224;
    bytes.copy_within(section..section + 40, section + 16);
    bytes[section..section + 16].fill(0);
    put16(&mut bytes, 0x84, 0x8664);
    put16(&mut bytes, 0x94, 240);
    put16(&mut bytes, optional, 0x20b);
    put64(&mut bytes, optional + 24, 0x1_4000_0000);
    bytes[0x200..].fill(0xcc);
    bytes[0x200..0x205].copy_from_slice(&[0xe8, 0x3b, 0, 0, 0]);
    bytes[0x210..0x217].copy_from_slice(&[0x48, 0x8b, 0x05, 0x29, 0, 0, 0]);
    bytes[0x220..0x222].copy_from_slice(&[0x48, 0xb8]);
    put64(&mut bytes, 0x222, 0x1_4000_1040);
    let mut editor = editor(bytes);
    editor
        .dispatch(Command::Goto(FileOffset::new(0x240)))
        .unwrap();
    editor.dispatch(Command::FindReferences).unwrap();
    assert_eq!(editor.cursor().get(), 0x240);
    assert!(matches!(
        editor.step_search(3).unwrap(),
        Some(SearchProgress::Pending { .. })
    ));
    editor.cancel_search();
    assert_eq!(editor.mode(), ViewMode::Hex);
    assert_eq!(editor.cursor().get(), 0x240);
    assert!(!editor.is_searching());
    editor.dispatch(Command::FindReferences).unwrap();
    for expected in [0x200, 0x210, 0x220] {
        assert_eq!(
            complete_reference_search(&mut editor, 3),
            SearchProgress::Found(FileOffset::new(expected)),
        );
        editor.dispatch(Command::NextReference).unwrap();
    }
    editor
        .document_mut()
        .overwrite(FileOffset::new(0x220), &[0xcc])
        .unwrap();
    assert!(editor.step_search(3).is_err());
    assert!(editor.dispatch(Command::NextReference).is_err());
    assert_eq!(editor.cursor().get(), 0x220);
}

#[test]
fn modes_share_cursor_and_pending_bytes() {
    let mut editor = raw(vec![0x90, 0xc3], DecodeMode::X86_64);
    editor.dispatch(Command::Goto(FileOffset::new(1))).unwrap();
    for mode in [ViewMode::Code, ViewMode::Text, ViewMode::Hex] {
        editor.dispatch(Command::CycleMode).unwrap();
        assert_eq!(editor.mode(), mode);
        assert_eq!(editor.cursor().get(), 1);
    }
    editor.dispatch(Command::BeginEdit).unwrap();
    editor.dispatch(Command::WriteByte(0xcc)).unwrap();
    editor.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
    assert_eq!(editor.code_rows(1).unwrap()[0].instruction.bytes(), &[0xcc]);
    editor.dispatch(Command::DiscardEdit).unwrap();
    assert_eq!(editor.code_rows(1).unwrap()[0].instruction.bytes(), &[0xc3]);
}

#[test]
fn ordinary_raw_decodes_as_x86_and_forced_raw_preserves_explicit_context() {
    let mut editor = editor(vec![0x90; 8]);
    assert_eq!(editor.code_rows(2).unwrap().len(), 2);
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::X86_32));
    assert!(editor.goto_expression(".1").is_err());
    editor.dispatch(Command::SetRaw(true)).unwrap();
    assert!(editor.code_rows(2).is_err());
    editor
        .dispatch(Command::SetDecodeMode(Some(DecodeMode::X86_64)))
        .unwrap();
    editor
        .dispatch(Command::SetRawBase(VirtualAddress::new(0x1000)))
        .unwrap();
    editor.goto_expression(".1003").unwrap();
    assert_eq!(editor.cursor().get(), 3);
    let rows = editor.code_rows(2).unwrap();
    assert_eq!(rows[0].instruction.address.get(), 0x1003);
    assert!(rows[0].mapped);
    assert!(editor.decode_context().raw && editor.decode_context().overridden);
    editor
        .dispatch(Command::SetEndian(Some(Endian::Big)))
        .unwrap();
    assert_eq!(
        editor.code_rows(1).unwrap()[0].instruction.status,
        DecodeStatus::Unsupported
    );
}

#[test]
fn dotted_goto_and_unmapped_failure_preserve_the_cursor() {
    let mut editor = editor(pe32());
    editor.goto_expression(".401006").unwrap();
    assert_eq!(editor.cursor().get(), 0x206);
    editor.goto_expression(".4198402t").unwrap();
    assert_eq!(editor.cursor().get(), 0x202);
    for input in [".401100", ".FFFFFFFFFFFFFFFF", ".+1", ".", ".1.2"] {
        let original = editor.cursor();
        assert!(editor.goto_expression(input).is_err(), "accepted {input}");
        assert_eq!(editor.cursor(), original);
    }
    assert_eq!(
        parse_virtual_address(".FFFFFFFFFFFFFFFF").unwrap().get(),
        u64::MAX
    );
}

#[test]
fn calculator_reads_virtual_and_file_positions_from_current_metadata() {
    let mut editor = editor(pe32());
    editor
        .dispatch(Command::Goto(FileOffset::new(0x200)))
        .unwrap();
    assert_eq!(editor.calculate("@o").unwrap(), 0x401000);
    assert_eq!(editor.calculate("@O").unwrap(), 0x200);
    editor
        .document_mut()
        .overwrite(FileOffset::new(0xB4), &0x500000u32.to_le_bytes())
        .unwrap();
    assert_eq!(editor.calculate("@o").unwrap(), 0x501000);
    assert_eq!(editor.cursor().get(), 0x200);
}

#[test]
fn edited_header_mapping_is_invalidated_for_undo_discard_and_save() {
    let mut editor = editor(pe32());
    editor.refresh_analysis();
    editor.dispatch(Command::BeginEdit).unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(0xb4), &0x500000u32.to_le_bytes())
        .unwrap();
    assert!(
        editor.executable().is_none(),
        "stale metadata escaped its generation"
    );
    assert!(editor.goto_expression(".401000").is_err());
    editor.goto_expression(".501000").unwrap();
    assert_eq!(editor.cursor().get(), 0x200);
    editor.dispatch(Command::Undo).unwrap();
    editor.goto_expression(".401000").unwrap();
    editor.dispatch(Command::Redo).unwrap();
    editor.goto_expression(".501000").unwrap();
    editor.dispatch(Command::DiscardEdit).unwrap();
    editor.goto_expression(".401000").unwrap();
    let mut saved = pe32();
    put32(&mut saved, 0xb4, 0x600000);
    editor.complete_save(MemorySource::new(saved));
    assert!(editor.executable().is_none());
    editor.goto_expression(".601000").unwrap();
    assert!(editor.goto_expression(".401000").is_err());
}

#[test]
fn branch_targets_are_redecoded_after_pending_edits_and_undo() {
    let mut editor = editor(pe32());
    editor.dispatch(Command::GotoEntry).unwrap();
    assert_eq!(
        editor.code_rows(1).unwrap()[0]
            .instruction
            .direct_branch
            .unwrap()
            .address
            .get(),
        0x401006
    );
    editor.dispatch(Command::BeginEdit).unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(0x201), &[2])
        .unwrap();
    editor.dispatch(Command::FollowBranch(1)).unwrap();
    assert_eq!(editor.cursor().get(), 0x204);
    editor.dispatch(Command::ReturnBranch).unwrap();
    editor.dispatch(Command::Undo).unwrap();
    editor.dispatch(Command::FollowBranch(1)).unwrap();
    assert_eq!(editor.cursor().get(), 0x206);
}

fn pe32_operand_navigation() -> Vec<u8> {
    let mut bytes = pe32();
    bytes[0x200..0x210].copy_from_slice(&[
        0xa1, 0x11, 0x10, 0x40, 0, // mov eax, [0x401011]
        0xb8, 0x14, 0x10, 0x40, 0, // mov eax, 0x401014
        0xff, 0x15, 0x20, 0x10, 0x40, 0, // call dword ptr [0x401020]
    ]);
    put32(&mut bytes, 0x220, 0x401030);
    bytes
}

#[test]
fn mapped_operands_follow_data_and_checked_pointer_contents_separately() {
    use crate::instructions::AddressOperand;

    let mut editor = editor(pe32_operand_navigation());
    editor
        .dispatch(Command::Goto(FileOffset::new(0x205)))
        .unwrap();
    editor.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
    let rows = editor.code_viewport(FileOffset::new(0x200), 3).unwrap();
    assert_eq!(rows[0].branch_number, Some(1));
    assert_eq!(
        rows[0].operand,
        Some(AddressOperand::Memory(VirtualAddress::new(0x401011)))
    );
    assert_eq!(
        rows[1].operand,
        Some(AddressOperand::Literal(VirtualAddress::new(0x401014)))
    );
    assert_eq!(
        rows[2].operand,
        Some(AddressOperand::Indirect {
            slot: VirtualAddress::new(0x401020),
            width: 4,
        })
    );
    assert_eq!(rows[2].indirect_target, Some(VirtualAddress::new(0x401030)));

    let display = editor.dispatch(Command::FollowBranch(1)).unwrap();
    assert_eq!(display, Effect::PositionViewport(FileOffset::new(0x211)));
    assert_eq!(editor.mode(), ViewMode::Hex);
    assert_eq!(editor.cursor(), FileOffset::new(0x211));
    assert!(editor.has_data_operand_return());
    let display = editor.dispatch(Command::ReturnBranch).unwrap();
    assert_eq!(display, Effect::PositionViewport(FileOffset::new(0x200)));
    assert_eq!(editor.mode(), ViewMode::Code);
    assert_eq!(editor.cursor(), FileOffset::new(0x205));
    assert!(!editor.has_data_operand_return());

    editor.code_viewport(FileOffset::new(0x200), 3).unwrap();
    editor.dispatch(Command::FollowBranch(3)).unwrap();
    assert_eq!(editor.cursor(), FileOffset::new(0x230));
    assert_eq!(editor.mode(), ViewMode::Code);
    assert!(!editor.has_data_operand_return());
    editor.dispatch(Command::ReturnBranch).unwrap();
    assert_eq!(editor.cursor(), FileOffset::new(0x205));
    assert!(!editor.document().is_dirty());

    editor
        .dispatch(Command::SetDecodeMode(Some(DecodeMode::X86_32)))
        .unwrap();
    editor.dispatch(Command::SetRaw(true)).unwrap();
    let rows = editor.code_viewport(FileOffset::new(0x200), 3).unwrap();
    assert!(rows[0].instruction.address_operand.is_some());
    assert!(rows.iter().all(|row| row.operand.is_none()));
    assert!(rows.iter().all(|row| row.branch_number.is_none()));
}

#[test]
fn indirect_pointer_is_revalidated_after_edits_and_rejects_unbacked_targets() {
    let mut editor = editor(pe32_operand_navigation());
    editor
        .dispatch(Command::Goto(FileOffset::new(0x20a)))
        .unwrap();
    editor.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
    editor.code_rows(1).unwrap();
    editor.dispatch(Command::BeginEdit).unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(0x220), &0x1234_u32.to_le_bytes())
        .unwrap();
    assert!(editor.dispatch(Command::FollowBranch(1)).is_err());
    assert_eq!(editor.cursor(), FileOffset::new(0x20a));
    assert_eq!(editor.mode(), ViewMode::Code);
    assert!(editor.dispatch(Command::ReturnBranch).is_err());
    assert!(editor.document().is_dirty());
    editor.dispatch(Command::Undo).unwrap();
    editor.dispatch(Command::FollowBranch(1)).unwrap();
    assert_eq!(editor.cursor(), FileOffset::new(0x230));
    assert!(!editor.document().is_dirty());
}

#[test]
fn indirect_pointer_slot_cannot_read_into_zero_fill() {
    let mut image = pe32_operand_navigation();
    let section = 0x98 + 0xe0;
    put32(&mut image, section + 8, 0x300);
    put32(&mut image, 0x20c, 0x4011fe);
    let mut editor = editor(image);
    editor
        .dispatch(Command::Goto(FileOffset::new(0x20a)))
        .unwrap();
    let rows = editor.code_rows(1).unwrap();
    assert_eq!(rows[0].branch_number, Some(1));
    assert_eq!(rows[0].indirect_target, None);
    assert!(editor.dispatch(Command::FollowBranch(1)).is_err());
    assert!(editor.dispatch(Command::ReturnBranch).is_err());
    assert_eq!(editor.cursor(), FileOffset::new(0x20a));
}

#[test]
fn restoring_header_bytes_refreshes_mappings_and_keeps_independent_undo() {
    for (restore, start) in [
        (Command::RestoreByte, 0xb6),
        (Command::RestorePreviousByte, 0xb7),
    ] {
        let mut editor = editor(pe32());
        editor.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
        editor.dispatch(Command::BeginEdit).unwrap();
        editor
            .dispatch(Command::Goto(FileOffset::new(0xb6)))
            .unwrap();
        editor.dispatch(Command::WriteByte(0x50)).unwrap();
        editor.goto_expression(".501000").unwrap();
        assert!(editor.goto_expression(".401000").is_err());
        assert_eq!(
            editor.executable().unwrap().image_base,
            Some(VirtualAddress::new(0x500000))
        );

        editor
            .dispatch(Command::Goto(FileOffset::new(start)))
            .unwrap();
        let generation = editor.document().generation();
        editor.dispatch(restore).unwrap();
        assert_eq!(editor.cursor().get(), 0xb6);
        assert!(editor.document().generation() > generation);
        assert!(editor.executable().is_none());
        assert!(editor.goto_expression(".501000").is_err());
        editor.goto_expression(".401000").unwrap();
        assert_eq!(editor.cursor().get(), 0x200);
        assert!(!editor.document().is_dirty());
        assert!(editor.is_editing());
        assert_eq!(editor.mode(), ViewMode::Code);

        editor.dispatch(Command::Undo).unwrap();
        assert!(editor.goto_expression(".401000").is_err());
        editor.goto_expression(".501000").unwrap();
        assert!(editor.document().is_dirty());
        editor.dispatch(Command::Redo).unwrap();
        assert!(editor.goto_expression(".501000").is_err());
        editor.goto_expression(".401000").unwrap();
        assert!(!editor.document().is_dirty());
    }
}

#[test]
fn restoring_branch_bytes_refreshes_code_and_targets_without_reverting_other_edits() {
    for (restore, start) in [
        (Command::RestoreByte, 0x201),
        (Command::RestorePreviousByte, 0x202),
    ] {
        let mut editor = editor(pe32());
        editor.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
        editor.dispatch(Command::BeginEdit).unwrap();
        editor
            .dispatch(Command::Goto(FileOffset::new(0x201)))
            .unwrap();
        editor.dispatch(Command::WriteByte(2)).unwrap();
        editor
            .dispatch(Command::Goto(FileOffset::new(0x207)))
            .unwrap();
        editor.dispatch(Command::WriteByte(0xcc)).unwrap();
        editor.dispatch(Command::GotoEntry).unwrap();
        assert_eq!(
            editor.code_rows(1).unwrap()[0].instruction.bytes(),
            &[0xeb, 2]
        );
        editor.dispatch(Command::FollowBranch(1)).unwrap();
        assert_eq!(editor.cursor().get(), 0x204);
        editor.dispatch(Command::ReturnBranch).unwrap();

        editor
            .dispatch(Command::Goto(FileOffset::new(start)))
            .unwrap();
        editor.dispatch(restore).unwrap();
        assert_eq!(editor.cursor().get(), 0x201);
        assert!(editor.executable().is_none());
        editor
            .dispatch(Command::Goto(FileOffset::new(0x200)))
            .unwrap();
        // Follow immediately, without a render/refresh of the restored bytes.
        editor.dispatch(Command::FollowBranch(1)).unwrap();
        assert_eq!(editor.cursor().get(), 0x206);
        editor.dispatch(Command::ReturnBranch).unwrap();
        let row = editor.code_rows(1).unwrap().remove(0);
        assert_eq!(row.instruction.bytes(), &[0xeb, 4]);
        assert_eq!(
            row.instruction.direct_branch.unwrap().address.get(),
            0x401006
        );

        for (command, target) in [(Command::Undo, 0x204), (Command::Redo, 0x206)] {
            editor.dispatch(command).unwrap();
            editor.dispatch(Command::FollowBranch(1)).unwrap();
            assert_eq!(editor.cursor().get(), target);
            editor.dispatch(Command::ReturnBranch).unwrap();
        }
        let mut bytes = [0; 8];
        editor
            .document_mut()
            .read_at(FileOffset::new(0x200), &mut bytes)
            .unwrap();
        assert_eq!(bytes, [0xeb, 4, 0x90, 0x90, 0x90, 0x90, 0xc3, 0xcc]);
        let patches = editor.document().patches();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].offset, FileOffset::new(0x207));
        assert_eq!(patches[0].bytes, [0xcc]);
        assert!(editor.is_editing());
        assert_eq!(editor.mode(), ViewMode::Code);
    }
}

#[test]
fn unmapped_padding_does_not_fabricate_followable_virtual_branches() {
    let mut bytes = pe32();
    bytes[0x300..0x302].copy_from_slice(&[0xeb, 0xfe]);
    let mut editor = editor(bytes);
    editor
        .dispatch(Command::Goto(FileOffset::new(0x300)))
        .unwrap();
    let row = editor.code_rows(1).unwrap().remove(0);
    assert!(!row.mapped);
    assert_eq!(row.instruction.address.get(), 0x300);
    assert_eq!(row.branch_number, None);
    assert_eq!(row.instruction.direct_branch, None);
    assert!(editor.dispatch(Command::FollowBranch(1)).is_err());
    assert_eq!(editor.cursor().get(), 0x300);
}

#[test]
fn unavailable_machine_and_malformed_headers_keep_raw_editing_available() {
    let mut bytes = pe32();
    put16(&mut bytes, 0x84, 0x9999);
    let mut editor = editor(bytes);
    editor.refresh_analysis();
    assert_eq!(editor.executable().unwrap().machine, Machine::Other(0x9999));
    assert!(editor.code_rows(1).is_err());
    editor
        .dispatch(Command::SetDecodeMode(Some(DecodeMode::X86_32)))
        .unwrap();
    editor.dispatch(Command::GotoEntry).unwrap();
    assert_eq!(
        editor.code_rows(1).unwrap()[0].instruction.status,
        DecodeStatus::Decoded
    );
    editor.dispatch(Command::BeginEdit).unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(0x3c), &u32::MAX.to_le_bytes())
        .unwrap();
    editor.refresh_analysis();
    assert!(editor.analysis_problem().is_some());
    assert!(editor.executable().is_none());
    editor.dispatch(Command::WriteByte(0xcc)).unwrap();
    assert!(editor.document().is_dirty());
    assert_eq!(
        editor.code_rows(1).unwrap()[0].instruction.address.get(),
        0x201
    );
}

#[test]
fn instruction_walk_retraces_known_boundaries_and_has_a_bounded_fallback() {
    let mut editor = raw(vec![0xb8, 1, 0, 0, 0, 0x90, 0xc3], DecodeMode::X86_32);
    for (count, expected) in [(1, 5), (1, 6), (-1, 5), (-1, 0)] {
        editor.dispatch(Command::MoveCode(count)).unwrap();
        assert_eq!(editor.cursor().get(), expected);
    }
    editor.dispatch(Command::Goto(FileOffset::new(6))).unwrap();
    editor.dispatch(Command::MoveCode(-1)).unwrap();
    assert_eq!(editor.cursor().get(), 5);
    assert!(editor.dispatch(Command::MoveCode(i32::MIN)).is_err());
    assert_eq!(editor.cursor().get(), 5);
}

#[test]
fn branch_history_and_viewport_have_explicit_limits() {
    let mut editor = raw(vec![0xeb, 0xfe], DecodeMode::X86_32);
    for _ in 0..MAX_NAVIGATION_HISTORY + 5 {
        editor.dispatch(Command::FollowBranch(1)).unwrap();
    }
    for _ in 0..MAX_NAVIGATION_HISTORY {
        editor.dispatch(Command::ReturnBranch).unwrap();
    }
    assert!(editor.dispatch(Command::ReturnBranch).is_err());
    assert!(editor.dispatch(Command::FollowBranch(0)).is_err());
    assert!(editor.dispatch(Command::FollowBranch(10)).is_err());
    assert!(editor.code_rows(MAX_CODE_ROWS + 1).is_err());
}

#[test]
fn raw_address_overflow_is_reported_without_wrapping_file_offsets() {
    let mut editor = raw(vec![0x90; 3], DecodeMode::X86_64);
    editor
        .dispatch(Command::SetRawBase(VirtualAddress::new(u64::MAX - 1)))
        .unwrap();
    assert!(editor.code_rows(3).is_err());
    assert_eq!(editor.cursor().get(), 0);
    assert!(editor.goto_expression(".0").is_err());
    assert_eq!(editor.cursor().get(), 0);
}

#[test]
fn selecting_a_fat_slice_clears_the_previous_raw_base() {
    let image = arm64_macho();
    let mut bytes = vec![0; 0x1000 + image.len()];
    for (offset, value) in [
        (0, 0xcafebabeu32),
        (4, 1),
        (8, 0x0100000c),
        (16, 0x1000),
        (20, image.len() as u32),
        (24, 12),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }
    bytes[0x1000..].copy_from_slice(&image);
    let mut editor = editor(bytes);
    editor
        .dispatch(Command::SetRawBase(VirtualAddress::new(0x8000)))
        .unwrap();
    editor.dispatch(Command::SelectSlice(0)).unwrap();
    let context = editor.decode_context();
    assert!(!context.raw && !context.overridden);
    // Mach-O mappings declare VAs rather than a PE-style RVA image base.
    assert_eq!(context.base.get(), 0);
    editor.dispatch(Command::GotoEntry).unwrap();
    assert_eq!(editor.cursor().get(), 0x1200);
    assert_eq!(
        editor.code_rows(1).unwrap()[0].instruction.address.get(),
        0x100000200
    );
}

#[test]
fn unmapped_to_mapped_transition_preserves_the_first_instruction_in_a_region() {
    let mut bytes = pe32();
    // Headers stop at0x1F0, leaving a raw gap before the .text mapping.
    put32(&mut bytes, 0x98 + 60, 0x1f0);
    bytes[0x1ff] = 0xb8; // A truncated mov must not consume the mapped entry bytes.
    let mut editor = editor(bytes);
    editor
        .dispatch(Command::Goto(FileOffset::new(0x1ff)))
        .unwrap();
    let rows = editor.code_rows(2).unwrap();
    assert_eq!(rows[0].instruction.length, 1);
    assert!(!rows[0].mapped);
    assert_eq!(rows[1].offset.get(), 0x200);
    assert_eq!(rows[1].instruction.address.get(), 0x401000);
    assert!(rows[1].mapped);
}

#[test]
fn displayed_branch_labels_reach_letters_and_survive_cursor_movement() {
    let mut editor = editor([0xEB, 0].repeat(14));
    let rows = editor.code_viewport(FileOffset::new(0), 12).unwrap();
    assert_eq!(rows[9].branch_number, Some(10));
    editor.dispatch(Command::MoveCode(1)).unwrap();
    assert_eq!(editor.cursor(), FileOffset::new(2));
    editor.dispatch(Command::FollowBranch(10)).unwrap();
    assert_eq!(editor.cursor(), FileOffset::new(20));
    editor.dispatch(Command::ReturnBranch).unwrap();
    assert_eq!(editor.cursor(), FileOffset::new(2));
}

#[test]
fn profile_changes_and_resync_drop_numbered_windows() {
    for change in [
        Command::SetDecodeMode(Some(DecodeMode::X86_16)),
        Command::SetRawBase(VirtualAddress::new(0x100)),
        Command::ResyncCode,
    ] {
        let mut editor = raw(vec![0xeb, 0, 0xeb, 2, 0xcc, 0xcc, 0xc3], DecodeMode::X86_32);
        let rows = editor.code_viewport(FileOffset::new(0), 2).unwrap();
        assert_eq!(rows[0].branch_number, Some(1));
        assert_eq!(rows[1].branch_number, Some(2));
        editor.dispatch(Command::Goto(FileOffset::new(2))).unwrap();
        editor.dispatch(change).unwrap();
        editor.dispatch(Command::FollowBranch(1)).unwrap();
        assert_eq!(editor.cursor(), FileOffset::new(6));
    }
}

#[test]
fn architectural_x86_wrap_never_wraps_the_raw_file_offset() {
    for (mode, limit) in [
        (DecodeMode::X86_16, u64::from(u16::MAX)),
        (DecodeMode::X86_32, u64::from(u32::MAX)),
        (DecodeMode::X86_64, u64::MAX),
    ] {
        let mut editor = raw(vec![0xeb, 0], mode);
        editor
            .dispatch(Command::SetRawBase(VirtualAddress::new(limit - 1)))
            .unwrap();
        let rows = editor.code_viewport(FileOffset::new(0), 1).unwrap();
        let row = &rows[0];
        assert_eq!(row.instruction.bytes(), &[0xeb, 0]);
        assert_eq!(row.instruction.direct_branch.unwrap().address.get(), 0);
        assert_eq!(row.branch_number, Some(1));
        assert!(editor.dispatch(Command::FollowBranch(1)).is_err());
        assert_eq!(editor.cursor(), FileOffset::new(0));
    }
}

#[test]
fn compact_visual_padding_preserves_plain_disassembly_and_instruction_boundaries() {
    let mut editor = editor(vec![0x90, 0x90, 0x90, 0xCC, 0xCC, 0xC3]);
    let rows = editor.code_rows(5).unwrap();
    assert_eq!(rows[1].offset, FileOffset::new(1));
    assert_eq!(rows[1].instruction.bytes(), [0x90]);
    assert_eq!(rows[4].offset, FileOffset::new(4));
    assert_eq!(rows[4].instruction.bytes(), [0xCC]);
    let rows = editor.code_viewport(FileOffset::new(0), 3).unwrap();
    assert_eq!(rows[0].instruction.bytes(), [0x90, 0x90, 0x90]);
    assert_eq!(rows[1].offset, FileOffset::new(3));
    assert_eq!(rows[1].instruction.bytes(), [0xCC, 0xCC]);
    assert_eq!(rows[2].offset, FileOffset::new(5));
    editor.dispatch(Command::Move(1)).unwrap();
    editor.dispatch(Command::MoveCode(1)).unwrap();
    assert_eq!(editor.cursor(), FileOffset::new(3));
    editor.dispatch(Command::MoveCode(-1)).unwrap();
    assert_eq!(editor.cursor(), FileOffset::new(0));
}

#[test]
fn branch_numbers_are_limited_to_the_requested_visible_window() {
    let mut editor = raw(vec![0x90, 0xeb, 0xfd], DecodeMode::X86_32);
    assert_eq!(editor.code_rows(1).unwrap()[0].branch_number, None);
    assert!(editor.dispatch(Command::FollowBranch(1)).is_err());
    assert_eq!(editor.cursor().get(), 0);
    assert_eq!(editor.code_rows(2).unwrap()[1].branch_number, Some(1));
    editor.dispatch(Command::FollowBranch(1)).unwrap();
    assert_eq!(editor.cursor().get(), 0);
}

#[test]
fn instruction_navigation_does_not_shrink_the_shared_branch_window() {
    let mut editor = raw(vec![0x90, 0x90, 0xeb, 0xfc], DecodeMode::X86_32);
    editor.code_rows(4).unwrap();
    editor.dispatch(Command::MoveCode(1)).unwrap();
    assert_eq!(editor.cursor().get(), 1);
    editor.dispatch(Command::FollowBranch(1)).unwrap();
    assert_eq!(editor.cursor().get(), 0);
}

#[test]
fn pe_thumb_machine_tags_select_thumb_without_an_odd_entry_address() {
    for machine in [0x1c2, 0x1c4] {
        for entry in [0x1000, 0x1001] {
            let mut bytes = pe32();
            put16(&mut bytes, 0x84, machine);
            put32(&mut bytes, 0x98 + 16, entry);
            bytes[0x200..0x202].copy_from_slice(&[0, 0xbf]);
            let mut editor = editor(bytes);
            editor.dispatch(Command::GotoEntry).unwrap();
            assert_eq!(editor.cursor().get(), 0x200);
            assert_eq!(editor.executable().unwrap().machine, Machine::Thumb);
            assert_eq!(editor.decode_context().mode, Some(DecodeMode::Thumb));
            let rows = editor.code_rows(1).unwrap();
            assert_eq!(rows[0].instruction.length, 2);
            assert_eq!(rows[0].instruction.status, DecodeStatus::Decoded);
        }
    }
}

#[test]
fn arm_blx_follow_and_return_restore_instruction_mode_and_raw_base() {
    let mut editor = raw(
        vec![0, 0, 0, 0xfa, 0, 0, 0, 0, 0, 0xbf, 0x70, 0x47],
        DecodeMode::Arm,
    );
    editor
        .dispatch(Command::SetRawBase(VirtualAddress::new(0x1000)))
        .unwrap();
    editor.code_rows(2).unwrap();
    editor.dispatch(Command::FollowBranch(1)).unwrap();
    assert_eq!(editor.cursor().get(), 8);
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::Thumb));
    let row = editor.code_rows(1).unwrap().remove(0);
    assert_eq!(row.instruction.length, 2);
    assert_eq!(row.instruction.address.get(), 0x1008);
    editor.dispatch(Command::ReturnBranch).unwrap();
    assert_eq!(editor.cursor().get(), 0);
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::Arm));
    assert_eq!(editor.decode_context().base.get(), 0x1000);
}

#[test]
fn code_view_at_a_large_offset_only_reads_bounded_instruction_windows() {
    #[derive(Debug)]
    struct LargeSource {
        read_bytes: usize,
    }
    impl ByteSource for LargeSource {
        fn len(&self) -> u64 {
            u64::MAX
        }
        fn read_at(&mut self, offset: FileOffset, out: &mut [u8]) -> std::io::Result<usize> {
            let count = out
                .len()
                .min((u64::MAX - offset.get()).min(out.len() as u64) as usize);
            out[..count].fill(0x90);
            self.read_bytes += count;
            Ok(count)
        }
    }
    let mut editor = Editor::new(Document::new(LargeSource { read_bytes: 0 }), true);
    editor.dispatch(Command::SetRaw(true)).unwrap();
    editor
        .dispatch(Command::SetDecodeMode(Some(DecodeMode::X86_64)))
        .unwrap();
    editor
        .dispatch(Command::Goto(FileOffset::new(1 << 40)))
        .unwrap();
    let rows = editor.code_rows(20).unwrap();
    assert_eq!(rows.len(), 20);
    assert_eq!(rows[0].offset.get(), 1 << 40);
    assert_eq!(rows[19].instruction.address.get(), (1 << 40) + 19);
    assert!(editor.document().source().read_bytes <= 20 * 15);
}
