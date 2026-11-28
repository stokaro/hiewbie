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
fn compact_visual_nops_preserve_plain_disassembly_and_instruction_boundaries() {
    let mut editor = editor(vec![0x90, 0x90, 0x90, 0xC3]);
    let rows = editor.code_rows(3).unwrap();
    assert_eq!(rows[1].offset, FileOffset::new(1));
    assert_eq!(rows[1].instruction.bytes(), [0x90]);
    let rows = editor.code_viewport(FileOffset::new(0), 3).unwrap();
    assert_eq!(rows[0].instruction.bytes(), [0x90, 0x90, 0x90]);
    assert_eq!(rows[1].offset, FileOffset::new(3));
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
