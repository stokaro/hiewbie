//! Independently generated fixtures for Mach-O command and relocation metadata.

use super::*;
use crate::MemorySource;

use crate::macho_fixtures as fixtures;

use fixtures::{
    CONTENT, ENTRY, RELOCATIONS, SYMBOLS, header_size, object, put32, put64, section_at,
    segment_mapping, segment_size, unix_thread,
};

fn analyze(bytes: Vec<u8>) -> Result<Executable, FormatError> {
    inspect(&mut Document::new(MemorySource::new(bytes)), None).map(Option::unwrap)
}

fn record<'a>(image: &'a Executable, name: &str) -> &'a StructureRecord {
    image
        .records
        .iter()
        .find(|record| record.name == name)
        .unwrap()
}

#[test]
fn old_thread_entries_use_declared_cpu_word_order_and_exact_layout() {
    for (cpu, expected_machine, pc_offset) in [
        (7, Machine::X86, 40),
        (0x0100_0007, Machine::X86_64, 128),
        (12, Machine::Arm, 60),
        (0x0100_000c, Machine::Aarch64, 256),
    ] {
        for big in [false, true] {
            let image = analyze(unix_thread(cpu, big, false)).unwrap();
            assert_eq!(image.machine, expected_machine);
            assert_eq!(image.endian, if big { Endian::Big } else { Endian::Little });
            assert_eq!(image.entry, Some(VirtualAddress::new(0x1500)));
            assert_eq!(
                image.va_to_file(image.entry.unwrap()),
                Ok(FileOffset::new(ENTRY as u64))
            );
            let command = record(&image, "Command[1] LC_UNIXTHREAD");
            assert_eq!(command.status, StructureStatus::Interpreted);
            assert_eq!(command.target.unwrap().offset.get(), ENTRY as u64);
            let pc = image
                .fields
                .iter()
                .find(|field| field.name == "Command[1].Thread[0].PC")
                .unwrap();
            assert_eq!(
                pc.range.offset.get(),
                (header_size(cpu) + segment_size(cpu) + 16 + pc_offset) as u64
            );
            assert_eq!(pc.range.len, if cpu & 0x0100_0000 != 0 { 8 } else { 4 });
        }
    }
}

#[test]
fn arm_cpsr_supplies_execution_state_without_rewriting_pc_or_machine() {
    for big in [false, true] {
        for (thumb, expected) in [(false, EntryMode::Arm), (true, EntryMode::Thumb)] {
            let mut bytes = unix_thread(12, big, thumb);
            let thread = header_size(12) + segment_size(12);
            // ARM_THREAD_STATE32 is also explicitly assigned flavor 9.
            put32(&mut bytes, thread + 8, 9, big);
            let image = analyze(bytes).unwrap();
            assert_eq!(image.machine, Machine::Arm);
            assert_eq!(image.entry, Some(VirtualAddress::new(0x1500)));
            assert_eq!(image.entry_mode, Some(expected));
            assert!(
                record(&image, "Command[1] LC_UNIXTHREAD")
                    .detail
                    .contains("CPSR")
            );
        }
    }
}

#[test]
fn misaligned_thread_pc_is_visible_but_never_normalized_into_another_entry() {
    use crate::{Command, Editor};
    for (thumb, pc) in [(false, 0x1501), (false, 0x1502), (true, 0x1501)] {
        let mut bytes = unix_thread(12, false, thumb);
        let field = header_size(12) + segment_size(12) + 16 + 60;
        put32(&mut bytes, field, pc, false);
        let mut editor = Editor::new(Document::new(MemorySource::new(bytes.clone())), false);
        editor.dispatch(Command::Goto(FileOffset::new(40))).unwrap();
        editor.refresh_analysis();
        let image = editor.executable().unwrap();
        assert_eq!(image.machine, Machine::Arm);
        assert_eq!(image.entry.unwrap().get(), u64::from(pc));
        assert_eq!(
            image.entry_mode,
            Some(if thumb {
                EntryMode::Thumb
            } else {
                EntryMode::Arm
            })
        );
        assert_eq!(record(image, "Command[1] LC_UNIXTHREAD").target, None);
        let context = editor.decode_context();
        assert!(
            editor
                .dispatch(Command::GotoEntry)
                .unwrap_err()
                .to_string()
                .contains("misaligned")
        );
        assert_eq!(editor.cursor().get(), 40);
        assert_eq!(editor.decode_context(), context);
        assert_eq!(editor.document().source().as_slice(), bytes);
        assert!(!editor.document().is_dirty());
    }
}

#[test]
fn main_entries_and_old_thread_entries_share_exact_file_navigation() {
    for cpu in [7, 0x0100_0007, 12, 0x0100_000c] {
        for big in [false, true] {
            let image = analyze(fixtures::main_entry(cpu, big)).unwrap();
            assert_eq!(image.entry, Some(VirtualAddress::new(0x1500)));
            assert_eq!(
                record(&image, "Command[1] LC_MAIN")
                    .target
                    .unwrap()
                    .offset
                    .get(),
                ENTRY as u64
            );
        }
    }
}

#[test]
fn core_thread_contexts_and_unknown_flavors_do_not_invent_process_entries() {
    let cpu = 0x0100_000c;
    let at = header_size(cpu) + segment_size(cpu);
    let mut bytes = unix_thread(cpu, false, false);
    put32(&mut bytes, 12, 4, false);
    put32(&mut bytes, at, 4, false);
    let image = analyze(bytes).unwrap();
    assert_eq!(image.entry, None);
    assert_eq!(
        record(&image, "Command[1] LC_THREAD")
            .target
            .unwrap()
            .offset
            .get(),
        ENTRY as u64
    );

    let mut bytes = unix_thread(cpu, false, false);
    put32(&mut bytes, at + 8, 0x1234, false);
    let image = analyze(bytes).unwrap();
    assert_eq!(image.entry, None);
    assert_eq!(
        record(&image, "Command[1] LC_UNIXTHREAD").status,
        StructureStatus::Unsupported
    );
    assert!(image.notes.iter().any(|note| note.contains("flavor 4660")));
}

#[test]
fn malformed_thread_counts_and_command_sizes_are_bounded() {
    for cpu in [7, 0x0100_0007, 12, 0x0100_000c] {
        for count in [0, 1, u32::MAX] {
            let mut bytes = unix_thread(cpu, false, false);
            let at = header_size(cpu) + segment_size(cpu);
            put32(&mut bytes, at + 12, count, false);
            assert!(matches!(analyze(bytes), Err(FormatError::Invalid { .. })));
        }
        let mut bytes = unix_thread(cpu, false, false);
        let at = header_size(cpu) + segment_size(cpu);
        put32(&mut bytes, at + 4, 8, false);
        assert!(matches!(analyze(bytes), Err(FormatError::Invalid { .. })));
    }
}

#[test]
fn every_load_command_retains_bounded_raw_span_and_explicit_status() {
    let cpu = 0x0100_0007;
    let at = header_size(cpu) + segment_size(cpu);
    let mut bytes = fixtures::main_entry(cpu, true);
    put32(&mut bytes, at, 0x8123_4567, true);
    let image = analyze(bytes).unwrap();
    let commands = image
        .records
        .iter()
        .filter(|row| row.kind == StructureKind::LoadCommand)
        .collect::<Vec<_>>();
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].status, StructureStatus::Interpreted);
    assert_eq!(commands[1].status, StructureStatus::Unsupported);
    assert_eq!(
        commands[1].record,
        FileRange {
            offset: FileOffset::new(at as u64),
            len: 24
        }
    );
    assert_eq!(commands[1].target, None);
    assert!(commands[1].detail.contains("Required-by-dyld"));
}

#[test]
fn high_vm_backing_is_a_suffix_with_no_file_offset_for_zero_fill() {
    for (cpu, pc_offset) in [(7, 40), (0x0100_0007, 128), (12, 60), (0x0100_000c, 256)] {
        for big in [false, true] {
            let mut bytes = unix_thread(cpu, big, false);
            segment_mapping(&mut bytes, cpu, big, 0x1000, 0x300, 0x400, 0x100, 1);
            let pc = header_size(cpu) + segment_size(cpu) + 16 + pc_offset;
            if cpu & 0x0100_0000 != 0 {
                put64(&mut bytes, pc, 0x1208, big);
            } else {
                put32(&mut bytes, pc, 0x1208, big);
            }
            let image = analyze(bytes).unwrap();
            for address in [0x1000, 0x11ff] {
                assert_eq!(
                    image.va_to_file(VirtualAddress::new(address)),
                    Err(AddressError::ZeroFill)
                );
            }
            assert_eq!(
                image.va_to_file(VirtualAddress::new(0x1200)),
                Ok(FileOffset::new(0x400))
            );
            assert_eq!(
                image.va_to_file(VirtualAddress::new(0x12ff)),
                Ok(FileOffset::new(0x4ff))
            );
            assert_eq!(
                image.va_to_file(VirtualAddress::new(0x1300)),
                Err(AddressError::Unmapped)
            );
            assert_eq!(
                image.file_to_va(FileOffset::new(0x400)),
                Ok(VirtualAddress::new(0x1200))
            );
            assert_eq!(
                image.va_to_file(image.entry.unwrap()),
                Ok(FileOffset::new(0x408))
            );
        }
    }
}

#[test]
fn empty_high_vm_backing_and_invalid_segment_extent_are_distinguished() {
    let cpu = 0x0100_0007;
    let mut bytes = unix_thread(cpu, false, false);
    segment_mapping(&mut bytes, cpu, false, 0x1000, 0x1000, 0x400, 0, 1);
    let image = analyze(bytes).unwrap();
    assert_eq!(
        image.va_to_file(VirtualAddress::new(0x1500)),
        Err(AddressError::ZeroFill)
    );
    assert_eq!(record(&image, "Command[1] LC_UNIXTHREAD").target, None);
    let mut bytes = unix_thread(cpu, false, false);
    segment_mapping(&mut bytes, cpu, false, 0x1000, 0x100, 0x400, 0x101, 1);
    assert!(matches!(analyze(bytes), Err(FormatError::Invalid { .. })));
}

#[test]
fn section_fields_and_object_targets_use_declared_section_file_layout() {
    for cpu in [7, 0x0100_0007] {
        for big in [false, true] {
            let mut bytes = object(cpu, big, &[]);
            let at = section_at(cpu, 0);
            let tail = if cpu & 0x0100_0000 != 0 { 48 } else { 40 };
            put32(&mut bytes, at + tail, (CONTENT + 4) as u32, big);
            let image = analyze(bytes).unwrap();
            let section = record(&image, "Section[1] __TEXT,__text");
            assert_eq!(section.record.offset.get(), at as u64);
            assert_eq!(section.target.unwrap().offset.get(), (CONTENT + 4) as u64);
            assert_eq!(
                image.va_to_file(VirtualAddress::new(0x1000)),
                Ok(FileOffset::new((CONTENT + 4) as u64))
            );
            assert_eq!(
                image.file_to_va(FileOffset::new((CONTENT + 4) as u64)),
                Ok(VirtualAddress::new(0x1000))
            );
            assert!(section.detail.contains("S_REGULAR"));
            assert!(section.detail.contains("instructions"));
            let flags = image
                .fields
                .iter()
                .find(|field| field.name == "Section[1].Flags")
                .unwrap();
            assert_eq!(flags.value, 0x8000_0400);
            assert_eq!(flags.range.offset.get(), (at + tail + 16) as u64);
        }
    }
}

#[test]
fn zero_fill_sections_and_mismatched_optional_backing_have_no_target() {
    let cpu = 0x0100_0007;
    let mut bytes = object(cpu, false, &[]);
    let at = section_at(cpu, 1);
    put32(&mut bytes, at + 64, 1, false);
    put32(&mut bytes, at + 48, u32::MAX, false);
    let image = analyze(bytes).unwrap();
    assert_eq!(record(&image, "Section[2] __DATA,__data").target, None);
    assert_eq!(
        image.va_to_file(VirtualAddress::new(0x1100)),
        Err(AddressError::ZeroFill)
    );

    let mut bytes = object(cpu, false, &[]);
    put32(&mut bytes, 12, 2, false);
    put32(
        &mut bytes,
        section_at(cpu, 0) + 48,
        (CONTENT + 4) as u32,
        false,
    );
    let image = analyze(bytes).unwrap();
    let row = record(&image, "Section[1] __TEXT,__text");
    assert_eq!(row.status, StructureStatus::Unsupported);
    assert_eq!(row.target, None);
    assert!(row.detail.contains("mappings disagree"));
}

#[test]
fn unknown_section_backing_stays_raw_and_object_header_alias_is_rejected() {
    let mut bytes = object(7, false, &[]);
    put32(&mut bytes, section_at(7, 0) + 56, 0xff, false);
    put32(&mut bytes, section_at(7, 0) + 40, u32::MAX, false);
    let image = analyze(bytes).unwrap();
    let row = record(&image, "Section[1] __TEXT,__text");
    assert_eq!(row.status, StructureStatus::Unsupported);
    assert_eq!(row.target, None);
    assert_eq!(
        image.va_to_file(VirtualAddress::new(0x1000)),
        Err(AddressError::Unmapped)
    );
    let mut bytes = object(7, false, &[]);
    segment_mapping(&mut bytes, 7, false, 0x1000, 0x600, 0, 0x600, 0);
    put32(&mut bytes, section_at(7, 0) + 40, 0, false);
    assert!(matches!(
        analyze(bytes),
        Err(FormatError::Invalid {
            message: "Mach-O object section overlaps its header or load commands",
            ..
        })
    ));
}

#[test]
fn dynamic_relocation_records_keep_loader_dependent_destinations_explicit() {
    for cpu in [7, 0x0100_0007] {
        let mut bytes = object(cpu, false, &[[4, 0x0c00_0001]]);
        let command = header_size(cpu) + segment_size(cpu) + 2 * fixtures::section_size(cpu) + 24;
        put32(&mut bytes, 16, 3, false);
        put32(
            &mut bytes,
            20,
            (command + 80 - header_size(cpu)) as u32,
            false,
        );
        let tail = if cpu & 0x0100_0000 != 0 { 48 } else { 40 };
        put32(&mut bytes, section_at(cpu, 0) + tail + 12, 0, false);
        for (relative, value) in [(0, 0xb), (4, 80), (64, RELOCATIONS as u32), (68, 1)] {
            put32(&mut bytes, command + relative, value, false);
        }
        let image = analyze(bytes).unwrap();
        let row = record(&image, "Dysymtab.External.Relocation[0]");
        assert_eq!(row.status, StructureStatus::Unsupported);
        assert_eq!(row.record.offset.get(), RELOCATIONS as u64);
        assert_eq!(row.target, None);
        assert!(row.detail.contains("dynamic relocation base"));
        assert!(
            record(&image, "Dysymtab.External.Relocation[0].Reference")
                .detail
                .contains("_target")
        );
        assert_eq!(
            record(&image, "Command[2] LC_DYSYMTAB").status,
            StructureStatus::Unsupported
        );
    }
}

#[test]
fn plain_relocations_decode_both_bitfield_orders_and_nlist_references() {
    // These are literal on-disk encodings: big-endian symbolnum is the upper
    // 24 bits, little-endian symbolnum the lower 24 bits.
    for (cpu, big, word1, width, nlist_offset) in [
        (7, false, 0x0c00_0001, 4, 0x70c),
        (7, true, 0x0000_0150, 4, 0x70c),
        (0x0100_0007, false, 0x0e00_0001, 8, 0x710),
        (0x0100_0007, true, 0x0000_0170, 8, 0x710),
        (0x0100_0007, false, 0x2d00_0001, 4, 0x710),
        (0x0100_0007, true, 0x0000_01d2, 4, 0x710),
    ] {
        let offset = if width == 8 { 16 } else { 4 };
        let original = object(cpu, big, &[[offset, word1]]);
        let mut document = Document::new(MemorySource::new(original.clone()));
        let image = inspect(&mut document, None).unwrap().unwrap();
        let value = record(&image, "Section[1].Relocation[0]");
        assert_eq!(value.record.offset.get(), RELOCATIONS as u64);
        assert_eq!(
            value.target,
            Some(FileRange {
                offset: FileOffset::new(CONTENT as u64 + u64::from(offset)),
                len: width
            })
        );
        assert!(value.detail.contains(if width == 8 {
            "stored=0x1122334455667788"
        } else {
            "stored=0x12345678"
        }));
        if matches!(word1, 0x2d00_0001 | 0x0000_01d2) {
            assert!(value.detail.contains("X86_64_RELOC_BRANCH"));
            assert!(value.detail.contains("pcrel=true"));
        }
        let reference = record(&image, "Section[1].Relocation[0].Reference");
        assert_eq!(reference.target.unwrap().offset.get(), nlist_offset);
        assert!(reference.detail.contains("_target"));
        assert!(reference.detail.contains("n_sect=2 (__DATA,__data)"));
        assert_eq!(document.source().as_slice(), original);
        assert_eq!(document.generation(), 0);
    }
}

#[test]
fn local_and_scattered_references_preserve_section_and_pair_identity() {
    for big in [false, true] {
        let local = if big { 0x0000_0240 } else { 0x0400_0002 };
        let image = analyze(object(7, big, &[[4, local]])).unwrap();
        assert_eq!(
            record(&image, "Section[1].Relocation[0].Reference")
                .target
                .unwrap()
                .offset
                .get(),
            section_at(7, 1) as u64
        );
        let image = analyze(object(
            7,
            big,
            &[[0xa200_0004, 0x1100], [0xa100_0000, 0x1000]],
        ))
        .unwrap();
        assert!(
            record(&image, "Section[1].Relocation[0]")
                .detail
                .contains("GENERIC_RELOC_SECTDIFF")
        );
        assert!(
            record(&image, "Section[1].Relocation[0]")
                .detail
                .contains("paired with [1]")
        );
        assert_eq!(record(&image, "Section[1].Relocation[1]").target, None);
        assert_eq!(
            record(&image, "Section[1].Relocation[1].Reference")
                .target
                .unwrap()
                .offset
                .get(),
            section_at(7, 0) as u64
        );
        let pair_encoding = if big { 0x41 } else { 0x1400_0000 };
        let image = analyze(object(
            7,
            big,
            &[[0xa200_0004, 0x1100], [0x7000_0000, pair_encoding]],
        ))
        .unwrap();
        assert_eq!(
            record(&image, "Section[1].Relocation[1]").status,
            StructureStatus::Interpreted
        );
        assert_eq!(record(&image, "Section[1].Relocation[1]").target, None);
    }
}

#[test]
fn arm_half_pair_uses_instruction_width_and_payload_instead_of_fake_offset() {
    let image = analyze(object(
        12,
        false,
        &[[0x7c, 0x8600_0002], [0xabcd, 0x1000_0000]],
    ))
    .unwrap();
    let first = record(&image, "Section[1].Relocation[0]");
    assert_eq!(first.target.unwrap().len, 4);
    assert!(first.detail.contains("upper half of Thumb instruction"));
    let pair = record(&image, "Section[1].Relocation[1]");
    assert_eq!(pair.target, None);
    assert!(pair.detail.contains("pair payload=0xABCD"));
}

#[test]
fn subtractor_pairs_keep_both_references_and_allow_local_unsigned_partner() {
    for (cpu, first) in [(0x0100_0007, 0x5e00_0000), (0x0100_000c, 0x1e00_0000)] {
        for second in [0x0e00_0001, 0x0600_0002] {
            let image = analyze(object(cpu, false, &[[16, first], [16, second]])).unwrap();
            let a = record(&image, "Section[1].Relocation[0]");
            let b = record(&image, "Section[1].Relocation[1]");
            assert_eq!(a.status, StructureStatus::Interpreted);
            assert_eq!(a.target, b.target);
            assert!(a.detail.contains("SUBTRACTOR"));
            assert!(b.detail.contains("paired with [0]"));
            assert!(
                record(&image, "Section[1].Relocation[0].Reference")
                    .detail
                    .contains("_entry")
            );
        }
    }
}

#[test]
fn arm64_addend_is_signed_metadata_and_pair_flags_are_checked() {
    for next in [0x2d00_0001, 0x3d00_0001, 0x4c00_0001] {
        let image = analyze(object(0x0100_000c, false, &[[32, 0xa4ff_fffc], [32, next]])).unwrap();
        let first = record(&image, "Section[1].Relocation[0]");
        assert_eq!(first.status, StructureStatus::Interpreted);
        assert_eq!(first.target, None);
        assert!(first.detail.contains("signed addend=-4"));
        assert!(
            !image
                .records
                .iter()
                .any(|row| row.name == "Section[1].Relocation[0].Reference")
        );
    }
    for next in [0x3900_0001, 0x3500_0001, 0x3c00_0001] {
        let image = analyze(object(0x0100_000c, false, &[[32, 0xa4ff_fffc], [32, next]])).unwrap();
        assert_eq!(
            record(&image, "Section[1].Relocation[0]").status,
            StructureStatus::Unsupported
        );
        assert_eq!(
            record(&image, "Section[1].Relocation[1]").status,
            StructureStatus::Unsupported
        );
        assert_eq!(record(&image, "Section[1].Relocation[0]").target, None);
        assert_eq!(record(&image, "Section[1].Relocation[1]").target, None);
    }
}

#[test]
fn unknown_and_malformed_relocations_keep_raw_records_without_inferred_targets() {
    let image = analyze(object(0x0100_0007, false, &[[u32::MAX, 0xfeff_ffff]])).unwrap();
    let unknown = record(&image, "Section[1].Relocation[0]");
    assert_eq!(unknown.status, StructureStatus::Unsupported);
    assert_eq!(unknown.target, None);
    assert!(unknown.detail.contains("plain"));
    assert!(!unknown.detail.contains("scattered"));
    let image = analyze(object(0x0100_0007, false, &[[16, 0x5e00_0000]])).unwrap();
    assert_eq!(record(&image, "Section[1].Relocation[0]").target, None);
    assert_eq!(
        record(&image, "Section[1].Relocation[0]").status,
        StructureStatus::Unsupported
    );
    assert!(
        record(&image, "Section[1].Relocation[0]")
            .detail
            .contains("required following relocation is missing")
    );
    let image = analyze(object(7, false, &[[0xffff, 0x1000_0000]])).unwrap();
    assert_eq!(
        record(&image, "Section[1].Relocation[0]").status,
        StructureStatus::Unsupported
    );
    assert_eq!(record(&image, "Section[1].Relocation[0]").target, None);
}

#[test]
fn bad_reference_metadata_is_local_and_does_not_hide_image_mappings() {
    for word in [0x0c00_0002, 0x0400_0003] {
        let image = analyze(object(7, false, &[[4, word]])).unwrap();
        assert_eq!(
            record(&image, "Section[1].Relocation[0].Reference").status,
            StructureStatus::Unsupported
        );
        assert_eq!(
            image.va_to_file(VirtualAddress::new(0x1004)),
            Ok(FileOffset::new(0x404))
        );
    }
    let mut bytes = object(7, false, &[[4, 0x0c00_0001]]);
    put32(&mut bytes, SYMBOLS + 12, 1000, false);
    let image = analyze(bytes).unwrap();
    assert!(
        record(&image, "Section[1].Relocation[0].Reference")
            .detail
            .contains("string index")
    );
    let mut bytes = object(7, false, &[[4, 0x0c00_0001]]);
    let symtab = header_size(7) + segment_size(7) + 2 * fixtures::section_size(7);
    put32(&mut bytes, symtab + 8, 0x7ff, false);
    let image = analyze(bytes).unwrap();
    assert_eq!(
        record(&image, "Command[1] LC_SYMTAB").status,
        StructureStatus::Unsupported
    );
    assert_eq!(
        record(&image, "Section[1].Relocation[0].Reference").target,
        None
    );
}

#[test]
fn relocation_value_width_and_table_extents_are_bounded() {
    assert!(matches!(
        analyze(object(7, false, &[[0x7f, 0x0400_0002]])),
        Err(FormatError::Invalid { .. })
    ));
    assert!(matches!(
        analyze(object(0x0100_0007, false, &[[u32::MAX, 0x0600_0002]])),
        Err(FormatError::Invalid { .. })
    ));
    let mut bytes = object(7, false, &[[4, 0x0400_0002]]);
    put32(&mut bytes, section_at(7, 0) + 48, 0x7fc, false);
    assert!(matches!(analyze(bytes), Err(FormatError::Invalid { .. })));
    let mut bytes = object(7, false, &[]);
    put32(&mut bytes, section_at(7, 0) + 52, u32::MAX, false);
    assert!(matches!(analyze(bytes), Err(FormatError::Limit { .. })));
}

#[test]
fn universal_relocation_records_and_targets_keep_selected_slice_provenance() {
    for fat64 in [false, true] {
        for big in [false, true] {
            let mut document = Document::new(MemorySource::new(fixtures::fat_objects(fat64, big)));
            for index in [1, 0, 1] {
                let image = inspect(&mut document, Some(index)).unwrap().unwrap();
                let base = 0x1000 + index as u64 * 0x1000;
                assert_eq!(image.selected_slice, Some(index));
                assert_eq!(
                    image.machine,
                    if index == 0 {
                        Machine::X86_64
                    } else {
                        Machine::Aarch64
                    }
                );
                assert_eq!(
                    record(&image, "Section[1].Relocation[0]")
                        .record
                        .offset
                        .get(),
                    base + 0x600
                );
                assert_eq!(
                    record(&image, "Section[1].Relocation[0]")
                        .target
                        .unwrap()
                        .offset
                        .get(),
                    base + 0x410
                );
                assert_eq!(
                    record(&image, "Section[1].Relocation[0].Reference")
                        .target
                        .unwrap()
                        .offset
                        .get(),
                    base + 0x710
                );
                assert!(
                    image
                        .records
                        .iter()
                        .all(|row| image.slice.contains(row.record.offset))
                );
            }
        }
    }
}

#[test]
fn edited_relocations_rebuild_interpretation_at_new_generation() {
    let original = object(7, false, &[[4, 0x0c00_0001]]);
    let mut document = Document::new(MemorySource::new(original.clone()));
    let first = inspect(&mut document, None).unwrap().unwrap();
    document
        .overwrite(
            FileOffset::new((RELOCATIONS + 4) as u64),
            &0x0400_0002_u32.to_le_bytes(),
        )
        .unwrap();
    let edited = inspect(&mut document, None).unwrap().unwrap();
    assert_ne!(first.generation, edited.generation);
    assert!(
        record(&first, "Section[1].Relocation[0].Reference")
            .detail
            .contains("nlist[1]")
    );
    assert!(
        record(&edited, "Section[1].Relocation[0].Reference")
            .detail
            .contains("section[2]")
    );
    assert_eq!(document.source().as_slice(), original);
    document.discard();
    let restored = inspect(&mut document, None).unwrap().unwrap();
    assert!(
        record(&restored, "Section[1].Relocation[0].Reference")
            .detail
            .contains("nlist[1]")
    );
}

#[test]
fn fat_container_fields_cannot_exceed_the_shared_field_budget() {
    let count = 3275_u32;
    let commands = 72 + count * 80;
    let length = 32 + commands as usize;
    let mut thin = vec![0; length];
    for (at, value) in [
        (0, 0xfeed_facf),
        (4, 0x0100_0007),
        (12, 2),
        (16, 1),
        (20, commands),
        (32, 0x19),
        (36, commands),
        (96, count),
    ] {
        put32(&mut thin, at, value, false);
    }
    put64(&mut thin, 64, length as u64, false);
    put64(&mut thin, 80, length as u64, false);
    let image = analyze(thin.clone()).unwrap();
    assert_eq!(image.fields.len(), MAX_FORMAT_FIELDS - 2);
    let mut fat = vec![0; 0x1000 + length];
    for (at, value) in [
        (0, 0xcafe_babe),
        (4, 1),
        (8, 0x0100_0007),
        (16, 0x1000),
        (20, length as u32),
        (24, 12),
    ] {
        put32(&mut fat, at, value, true);
    }
    fat[0x1000..].copy_from_slice(&thin);
    assert!(matches!(
        analyze(fat),
        Err(FormatError::Limit {
            resource: "header fields",
            limit: MAX_FORMAT_FIELDS
        })
    ));
}
