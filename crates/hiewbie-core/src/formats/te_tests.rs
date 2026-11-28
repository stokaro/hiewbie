use super::*;
use crate::MemorySource;

use crate::te_fixtures as fixtures;

use fixtures::{
    BASE32, BASE64, BIAS, DEBUG, DEBUG_DATA, ENTRY, RELOCATION_TARGET, RELOCATIONS, put16, put32,
    put64, te, te32, te64,
};

fn analyze(bytes: Vec<u8>) -> Result<Executable, FormatError> {
    inspect(&mut Document::new(MemorySource::new(bytes)), None).map(|image| image.unwrap())
}

fn record<'a>(image: &'a Executable, prefix: &str) -> &'a StructureRecord {
    image
        .records
        .iter()
        .find(|record| record.name.starts_with(prefix))
        .unwrap()
}

#[test]
fn machine_selects_te_width_without_changing_the_shared_header_layout() {
    for (raw, machine, bits, format, base) in [
        (0x14c, Machine::X86, 32, BinaryFormat::Te32, BASE32),
        (0x8664, Machine::X86_64, 64, BinaryFormat::Te64, BASE64),
        (0x1c0, Machine::Arm, 32, BinaryFormat::Te32, BASE32),
        (0x1c2, Machine::Arm, 32, BinaryFormat::Te32, BASE32),
        (0x1c4, Machine::Thumb, 32, BinaryFormat::Te32, BASE32),
        (0xaa64, Machine::Aarch64, 64, BinaryFormat::Te64, BASE64),
        (
            0xfeed,
            Machine::Other(0xfeed),
            0,
            BinaryFormat::TeUnknown,
            BASE32,
        ),
    ] {
        let image = analyze(te(raw)).unwrap();
        assert_eq!(
            (image.machine, image.bitness, image.format),
            (machine, bits, format)
        );
        assert_eq!(image.endian, Endian::Little);
        assert_eq!(image.image_base, Some(VirtualAddress::new(base)));
        let entry_rva = if raw == 0x1c4 { 0x1011 } else { 0x1010 };
        assert_eq!(image.entry, Some(VirtualAddress::new(base + entry_rva)));
        assert_eq!(
            image.va_to_file(VirtualAddress::new(base + 0x1010)),
            Ok(FileOffset::new(ENTRY as u64))
        );
        let field = image
            .fields
            .iter()
            .find(|field| field.name == "TE.ImageBase")
            .unwrap();
        assert_eq!(
            field.range,
            FileRange {
                offset: FileOffset::new(16),
                len: 8
            }
        );
        assert_eq!(image.regions[1].header.offset, FileOffset::new(40));
    }
}

#[test]
fn uefi_mixed_machine_preserves_arm_family_and_entry_state_in_original_rva() {
    for (bytes, entry) in [(te(0x1c2), 0x1010), (fixtures::te_mixed_thumb(), 0x1011)] {
        let image = analyze(bytes).unwrap();
        assert_eq!(image.machine, Machine::Arm);
        assert_eq!(image.entry, Some(VirtualAddress::new(BASE32 + entry)));
        assert!(
            image
                .notes
                .iter()
                .any(|note| note.contains("ARMTHUMB_MIXED"))
        );
        assert!(
            record(&image, "Relocation[0:0]")
                .name
                .ends_with("THUMB_MOV32")
        );
    }
    let mut bytes = fixtures::te_mixed_thumb();
    put16(&mut bytes, RELOCATIONS + 8, 0x5020);
    let image = analyze(bytes).unwrap();
    assert!(
        record(&image, "Relocation[0:0]")
            .name
            .ends_with("ARM_MOV32")
    );
}

#[test]
fn stripped_translation_keeps_original_rvas_and_header_loaded_address() {
    let image = analyze(te32()).unwrap();
    assert_eq!(BIAS, 0xf8);
    assert_eq!(
        image.file_to_va(FileOffset::new(0)),
        Ok(VirtualAddress::new(0x4000f8))
    );
    assert_eq!(
        image.rva_to_va(Rva::new(0x1010)),
        Ok(VirtualAddress::new(0x401010))
    );
    assert_eq!(
        image.va_to_file(VirtualAddress::new(0x401010)),
        Ok(FileOffset::new(0xf18))
    );
    assert_eq!(
        image.file_to_va(FileOffset::new(0xf08)),
        Ok(VirtualAddress::new(0x401000))
    );
    assert_eq!(
        image.va_to_file(VirtualAddress::new(BASE32 + 0x2200)),
        Err(AddressError::ZeroFill)
    );
    assert_eq!(
        image.va_to_file(VirtualAddress::new(BASE32)),
        Err(AddressError::Unmapped)
    );
    assert_eq!(
        image.file_to_va(FileOffset::new(0x400)),
        Ok(VirtualAddress::new(0x4004f8))
    );
    assert_eq!(image.regions[0].file.len, 0xf08);
    assert_eq!(
        image.va_to_file(VirtualAddress::new(0x400fff)),
        Ok(FileOffset::new(0xf07))
    );
    let pointer = image
        .fields
        .iter()
        .find(|field| field.name == "Section[0].PointerToRawData")
        .unwrap();
    assert_eq!(pointer.value, 0x1000);
    assert_eq!(
        pointer.range,
        FileRange {
            offset: FileOffset::new(60),
            len: 4
        }
    );
}

#[test]
fn record_navigation_distinguishes_source_and_full_target_spans() {
    let image = analyze(te64()).unwrap();
    for (name, offset, len, target_offset, target_len) in [
        ("Section[0]", 40, 40, 0xf08, 0x100),
        ("TE.BaseRelocationDirectory", 24, 8, RELOCATIONS as u64, 12),
        (
            "Relocation[0:0]",
            RELOCATIONS as u64 + 8,
            2,
            RELOCATION_TARGET as u64,
            8,
        ),
        ("Debug[0]", DEBUG as u64, 28, DEBUG_DATA as u64, 40),
    ] {
        let record = record(&image, name);
        assert_eq!(
            record.record,
            FileRange {
                offset: FileOffset::new(offset),
                len
            }
        );
        assert_eq!(
            record.target,
            Some(FileRange {
                offset: FileOffset::new(target_offset),
                len: target_len
            })
        );
    }
    let unknown_relocation = record(&image, "Relocation[0:1]");
    assert_eq!(unknown_relocation.status, StructureStatus::Unsupported);
    assert_eq!(unknown_relocation.target, None);
    let unknown_debug = record(&image, "Debug[1]");
    assert_eq!(unknown_debug.status, StructureStatus::Unsupported);
    assert_eq!(unknown_debug.target.unwrap().len, 4);
    assert_eq!(
        record(&image, "DebugData[1]").status,
        StructureStatus::Unsupported
    );
}

#[test]
fn relocation_machine_types_select_exact_patch_widths() {
    for (machine, label, width) in [
        (0x14c, "HIGHLOW", 4),
        (0x8664, "DIR64", 8),
        (0x1c0, "ARM_MOV32", 8),
        (0x1c2, "THUMB_MOV32", 8),
        (0x1c4, "THUMB_MOV32", 8),
        (0xaa64, "DIR64", 8),
    ] {
        let image = analyze(te(machine)).unwrap();
        let relocation = record(&image, "Relocation[0:0]");
        assert!(relocation.name.ends_with(label));
        assert_eq!(relocation.target.unwrap().len, width);
    }
    let mut bytes = te64();
    put16(&mut bytes, RELOCATIONS + 8, 0x5020);
    let image = analyze(bytes).unwrap();
    let relocation = record(&image, "Relocation[0:0]");
    assert_eq!(relocation.status, StructureStatus::Unsupported);
    assert_eq!(relocation.target, None);
}

#[test]
fn highadj_consumes_its_signed_low_word_as_one_record() {
    let mut bytes = te32();
    put16(&mut bytes, RELOCATIONS + 8, 0x4020);
    put16(&mut bytes, RELOCATIONS + 10, 0xfedc);
    let image = analyze(bytes.clone()).unwrap();
    let relocation = record(&image, "Relocation[0:0]");
    assert_eq!(relocation.record.len, 4);
    assert_eq!(relocation.target.unwrap().len, 2);
    assert!(relocation.detail.contains("0xFEDC"));
    assert!(
        !image
            .records
            .iter()
            .any(|entry| entry.name.starts_with("Relocation[0:1]"))
    );
    put32(&mut bytes, 28, 10);
    put32(&mut bytes, RELOCATIONS + 4, 10);
    assert!(matches!(
        analyze(bytes),
        Err(FormatError::Invalid {
            message: "TE HIGHADJ relocation is missing its second slot",
            ..
        })
    ));
}

#[test]
fn padding_relocations_never_navigate_to_a_target() {
    let mut bytes = te32();
    put16(&mut bytes, RELOCATIONS + 8, 0x0fff);
    let image = analyze(bytes).unwrap();
    let relocation = record(&image, "Relocation[0:0]");
    assert_eq!(relocation.status, StructureStatus::Interpreted);
    assert_eq!(relocation.target, None);
    assert!(relocation.name.ends_with("ABSOLUTE padding"));
}

#[test]
fn pic_and_absent_relocations_are_distinct_without_reading_empty_targets() {
    let mut bytes = te64();
    put32(&mut bytes, 24, 1);
    put32(&mut bytes, 28, 0);
    let image = analyze(bytes.clone()).unwrap();
    assert!(
        image
            .notes
            .iter()
            .any(|note| note.contains("Position-independent"))
    );
    assert_eq!(record(&image, "TE.BaseRelocationDirectory").target, None);
    assert!(
        !image
            .records
            .iter()
            .any(|record| record.name.starts_with("RelocationBlock"))
    );
    put32(&mut bytes, 24, 0);
    let image = analyze(bytes).unwrap();
    assert!(
        !image
            .notes
            .iter()
            .any(|note| note.contains("Position-independent"))
    );
}

#[test]
fn zero_bias_and_maximum_stripped_size_use_checked_original_offsets() {
    for stripped in [40, 41, u16::MAX] {
        let mut bytes = te32();
        let bias = u32::from(stripped) - 40;
        // Disable directories to isolate the stripped/header/section mapping.
        bytes[24..40].fill(0);
        put16(&mut bytes, 6, stripped);
        for (at, raw_offset) in [(60, 0x1000 - BIAS as u32), (100, 0x2000 - BIAS as u32)] {
            put32(&mut bytes, at, raw_offset + bias);
        }
        put32(&mut bytes, 8, ENTRY as u32 + bias);
        put32(&mut bytes, 12, 0x1000 - BIAS as u32 + bias);
        put32(&mut bytes, 52, 0x1000 - BIAS as u32 + bias);
        put32(&mut bytes, 92, 0x2000 - BIAS as u32 + bias);
        let image = analyze(bytes).unwrap();
        assert_eq!(image.regions[0].address.get(), BASE32 + u64::from(bias));
        assert_eq!(
            image.va_to_file(image.entry.unwrap()),
            Ok(FileOffset::new(ENTRY as u64))
        );
    }
}

#[test]
fn retained_header_padding_excludes_data_before_code_and_its_zero_fill() {
    let mut bytes = te32();
    let data = 0x400 - BIAS;
    bytes.copy_within(RELOCATIONS..RELOCATIONS + 0x200, data);
    put32(&mut bytes, 92, 0x400);
    put32(&mut bytes, 100, 0x400);
    put32(&mut bytes, 24, 0x400);
    put32(&mut bytes, 32, 0x440);
    for (at, value) in [
        (data + 0x40 + 20, 0x4c0),
        (data + 0x40 + 24, 0x4c0),
        (data + 0x40 + 28 + 20, 0x500),
        (data + 0x40 + 28 + 24, 0x500),
    ] {
        put32(&mut bytes, at, value);
    }
    let image = analyze(bytes).unwrap();
    assert_eq!(
        image.va_to_file(VirtualAddress::new(BASE32 + 0x400)),
        Ok(FileOffset::new(data as u64))
    );
    assert_eq!(
        image.file_to_va(FileOffset::new((data + 0x40) as u64)),
        Ok(VirtualAddress::new(BASE32 + 0x440))
    );
    assert_eq!(
        image.va_to_file(VirtualAddress::new(BASE32 + 0x610)),
        Err(AddressError::ZeroFill)
    );
    assert_eq!(
        image.file_to_va(FileOffset::new(0x610 - BIAS as u64)),
        Err(AddressError::Unmapped)
    );
    assert_eq!(
        image.file_to_va(FileOffset::new(0x700 - BIAS as u64)),
        Ok(VirtualAddress::new(BASE32 + 0x700))
    );
    assert_eq!(
        record(&image, "TE.DebugDirectory").target.unwrap().offset,
        FileOffset::new((data + 0x40) as u64)
    );
}

#[test]
fn bss_and_file_padding_do_not_invent_virtual_file_bytes() {
    let mut bytes = te32();
    bytes[24..40].fill(0);
    put32(&mut bytes, 96, 0);
    put32(&mut bytes, 100, 0);
    put32(&mut bytes, 48, 0x80);
    let image = analyze(bytes).unwrap();
    assert_eq!(record(&image, "Section[1]").target, None);
    assert_eq!(
        image.va_to_file(VirtualAddress::new(BASE32 + 0x2000)),
        Err(AddressError::ZeroFill)
    );
    assert_eq!(
        image.file_to_va(FileOffset::new(0x1080 - BIAS as u64)),
        Err(AddressError::Unmapped)
    );
}

#[test]
fn debug_overlay_uses_adjusted_raw_pointer_without_a_virtual_mapping() {
    let mut bytes = te32();
    let overlay = bytes.len();
    bytes.extend_from_slice(b"opaque debug");
    put32(&mut bytes, DEBUG + 28 + 16, 12);
    put32(&mut bytes, DEBUG + 28 + 20, 0);
    put32(&mut bytes, DEBUG + 28 + 24, (overlay + BIAS) as u32);
    let image = analyze(bytes).unwrap();
    let debug = record(&image, "Debug[1]");
    assert_eq!(
        debug.target,
        Some(FileRange {
            offset: FileOffset::new(overlay as u64),
            len: 12
        })
    );
    assert_eq!(
        image.file_to_va(FileOffset::new(overlay as u64)),
        Err(AddressError::Unmapped)
    );
}

#[test]
fn directory_translation_uses_section_mapping_not_an_rva_file_shortcut() {
    let mut bytes = te32();
    // Move only the virtual .data section, retaining its physical bytes.
    put32(&mut bytes, 92, 0x3000);
    put32(&mut bytes, 24, 0x3000);
    put32(&mut bytes, 32, 0x3040);
    put32(&mut bytes, DEBUG + 20, 0x30c0);
    put32(&mut bytes, DEBUG + 28 + 20, 0x3100);
    let image = analyze(bytes).unwrap();
    assert_eq!(
        record(&image, "TE.BaseRelocationDirectory")
            .target
            .unwrap()
            .offset,
        FileOffset::new(RELOCATIONS as u64)
    );
    assert_eq!(
        record(&image, "Debug[0]").target.unwrap().offset,
        FileOffset::new(DEBUG_DATA as u64)
    );
}

#[test]
fn codeview_rsds_and_nb10_preserve_exact_prefix_fields_and_raw_payloads() {
    let image = analyze(te32()).unwrap();
    let data = record(&image, "DebugData[0]");
    assert_eq!(data.record.len, 40);
    assert_eq!(data.status, StructureStatus::Interpreted);
    assert!(data.detail.contains("RSDS"));
    assert!(
        image.fields.iter().any(
            |field| field.name == "Debug[0].CodeView.GuidWord[0]" && field.value == 0x0302_0100
        )
    );
    let mut bytes = te32();
    bytes[DEBUG_DATA..DEBUG_DATA + 4].copy_from_slice(b"NB10");
    put32(&mut bytes, DEBUG_DATA + 4, 0);
    put32(&mut bytes, DEBUG_DATA + 8, 0x1234_5678);
    put32(&mut bytes, DEBUG_DATA + 12, 9);
    let image = analyze(bytes.clone()).unwrap();
    assert!(
        record(&image, "DebugData[0]")
            .detail
            .contains("NB10; age 9")
    );
    bytes[DEBUG_DATA..DEBUG_DATA + 4].copy_from_slice(b"NEW!");
    let image = analyze(bytes).unwrap();
    assert_eq!(
        record(&image, "DebugData[0]").status,
        StructureStatus::Unsupported
    );
}

#[test]
fn malformed_headers_and_stripped_arithmetic_reject_without_mutation() {
    let mut cases = Vec::new();
    let mut bytes = te32();
    put16(&mut bytes, 6, 39);
    cases.push(bytes);
    let mut bytes = te32();
    bytes[4] = 0;
    cases.push(bytes);
    let mut bytes = te32();
    bytes[4] = u8::MAX;
    cases.push(bytes);
    let mut bytes = te32();
    put32(&mut bytes, 60, 1);
    cases.push(bytes);
    let mut bytes = te32();
    put32(&mut bytes, 60, BIAS as u32);
    cases.push(bytes);
    let mut bytes = te32();
    put32(&mut bytes, 8, 1);
    cases.push(bytes);
    let mut bytes = te32();
    put32(&mut bytes, 12, 1);
    cases.push(bytes);
    for code in [0, BIAS as u32 + 119, 0x3000] {
        let mut bytes = te32();
        put32(&mut bytes, 12, code);
        cases.push(bytes);
    }
    let mut bytes = te32();
    put32(&mut bytes, 52, 1);
    cases.push(bytes);
    let mut bytes = te32();
    put64(&mut bytes, 16, 1_u64 << 32);
    cases.push(bytes);
    let mut bytes = te64();
    put64(&mut bytes, 16, u64::MAX - 0x1000);
    cases.push(bytes);
    let mut bytes = te64();
    put32(&mut bytes, 52, u32::MAX - 0x80);
    cases.push(bytes);
    let mut bytes = te64();
    put32(&mut bytes, 60, u32::MAX - 0x80);
    cases.push(bytes);
    for bytes in cases {
        let mut document = Document::new(MemorySource::new(bytes.clone()));
        assert!(matches!(
            inspect(&mut document, None),
            Err(FormatError::Invalid { .. })
        ));
        assert_eq!(document.source().as_slice(), bytes);
        assert_eq!(document.generation(), 0);
        assert!(!document.is_dirty());
    }
}

#[test]
fn malformed_directories_relocation_blocks_and_debug_targets_reject() {
    let mut cases = Vec::new();
    for (at, value) in [
        (24, 0),
        (32, 0),
        (28, 0x201),
        (36, 55),
        (RELOCATIONS + 4, 0),
        (RELOCATIONS + 4, 7),
        (RELOCATIONS + 4, 11),
        (RELOCATIONS + 4, 16),
        (RELOCATIONS, 0x1001),
        (DEBUG + 24, 1),
        (DEBUG + 24, 0x20c1),
        (DEBUG + 16, u32::MAX),
        (DEBUG + 16, 20),
        (DEBUG + 20, 0x2210),
    ] {
        let mut bytes = te32();
        put32(&mut bytes, at, value);
        cases.push((at, value, bytes));
    }
    for (at, value, bytes) in cases {
        assert!(
            matches!(analyze(bytes), Err(FormatError::Invalid { .. })),
            "mutation at {at:X} value {value:X}"
        );
    }
}

#[test]
fn entire_relocation_and_debug_targets_must_be_uniquely_file_backed() {
    let mut bytes = te64();
    // Last two bytes are backed, but an eight-byte DIR64 target is not.
    put16(&mut bytes, RELOCATIONS + 8, 0xa0fe);
    assert!(matches!(analyze(bytes), Err(FormatError::Invalid { .. })));
    let mut bytes = te64();
    put32(&mut bytes, DEBUG + 20, 0x21fc);
    put32(&mut bytes, DEBUG + 24, 0x21fc);
    assert!(matches!(analyze(bytes), Err(FormatError::Invalid { .. })));
    let mut bytes = te64();
    // The added mapping overlaps only the middle of the relocation target.
    bytes[4] = 3;
    bytes[120..124].copy_from_slice(b"tiny");
    put32(&mut bytes, 128, 2);
    put32(&mut bytes, 132, 0x1023);
    put32(&mut bytes, 136, 2);
    put32(&mut bytes, 140, 0x1030);
    assert!(matches!(analyze(bytes), Err(FormatError::Invalid { .. })));
}

#[test]
fn unbacked_entry_is_retained_as_metadata_but_never_resolves_to_file_bytes() {
    let mut bytes = te32();
    put32(&mut bytes, 8, 0x2210);
    let image = analyze(bytes).unwrap();
    assert_eq!(
        image.va_to_file(image.entry.unwrap()),
        Err(AddressError::ZeroFill)
    );
}

#[test]
fn all_truncated_valid_fixture_prefixes_fail_without_panicking() {
    let bytes = te64();
    for len in 2..bytes.len() {
        assert!(
            inspect(
                &mut Document::new(MemorySource::new(bytes[..len].to_vec())),
                None
            )
            .is_err(),
            "prefix length {len}"
        );
    }
}

#[test]
fn table_work_and_retained_records_have_independent_bounds() {
    let mut bytes = te32();
    bytes[32..40].fill(0);
    let size = 8 + 2 * (MAX_FORMAT_ENTRIES + 1);
    bytes.resize(RELOCATIONS + size, 0);
    put32(&mut bytes, 88, size as u32);
    put32(&mut bytes, 96, size as u32);
    put32(&mut bytes, 28, size as u32);
    put32(&mut bytes, RELOCATIONS + 4, size as u32);
    assert!(matches!(
        analyze(bytes),
        Err(FormatError::Limit {
            resource: "table entries",
            ..
        })
    ));
    let mut bytes = te32();
    bytes[32..40].fill(0);
    let size = 8 * MAX_FORMAT_RECORDS;
    bytes.resize(RELOCATIONS + size, 0);
    put32(&mut bytes, 88, size as u32);
    put32(&mut bytes, 96, size as u32);
    put32(&mut bytes, 28, size as u32);
    for index in 0..MAX_FORMAT_RECORDS {
        put32(&mut bytes, RELOCATIONS + index * 8, 0x1000);
        put32(&mut bytes, RELOCATIONS + index * 8 + 4, 8);
    }
    assert!(matches!(
        analyze(bytes),
        Err(FormatError::Limit {
            resource: "structure records",
            ..
        })
    ));
}

#[test]
fn raw_patch_reopen_preserves_adjustment_fields_and_every_unrelated_byte() {
    for bytes in [te32(), te64(), te(0x1c4), te(0xaa64)] {
        let mut document = Document::new(MemorySource::new(bytes.clone()));
        let before = inspect(&mut document, None).unwrap().unwrap();
        document
            .overwrite(FileOffset::new(ENTRY as u64), &[0xcc])
            .unwrap();
        let mut saved = vec![0; bytes.len()];
        assert_eq!(
            document.read_at(FileOffset::new(0), &mut saved).unwrap(),
            bytes.len()
        );
        let mut expected = bytes;
        expected[ENTRY] = 0xcc;
        assert_eq!(saved, expected);
        let reopened = analyze(saved).unwrap();
        assert_eq!(reopened.fields, before.fields);
        assert_eq!(reopened.regions, before.regions);
        assert_eq!(reopened.records, before.records);
        assert_eq!(reopened.entry, before.entry);
    }
}
