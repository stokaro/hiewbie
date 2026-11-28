use super::*;
use crate::formats::{Endian, VirtualAddress};
use crate::instructions::{DecodeMode, DecodeStatus, decode_one};
use crate::{Command, Document, Editor, FileOffset, MemorySource};

#[test]
fn compiled_catalog_has_unique_valid_descriptions_in_stable_order() {
    let report = current();
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.version, env!("CARGO_PKG_VERSION"));
    assert!(report.capabilities.len() < MAX_CAPABILITIES);
    assert!(
        report
            .capabilities
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id)
    );
    let mut registry = Registry::new();
    for entry in report.capabilities.iter().rev() {
        registry.register(*entry).unwrap();
    }
    assert_eq!(registry.report().capabilities, report.capabilities);
}

#[test]
fn decoder_descriptions_match_actual_compiled_backends_and_orders() {
    for (id, mode, bytes) in [
        ("decode.x86-16", DecodeMode::X86_16, &[0x90][..]),
        ("decode.x86-32", DecodeMode::X86_32, &[0x90]),
        ("decode.x86-64", DecodeMode::X86_64, &[0x90]),
        ("decode.arm", DecodeMode::Arm, &[0, 0, 0xA0, 0xE1]),
        ("decode.thumb", DecodeMode::Thumb, &[0, 0xBF]),
        (
            "decode.aarch64",
            DecodeMode::Aarch64,
            &[0x1F, 0x20, 3, 0xD5],
        ),
    ] {
        let report = current();
        let entry = report.require(id).unwrap();
        assert_eq!(entry.category, Category::Decode);
        assert_eq!(entry.status, Status::Limited);
        let backend = entry.backend.unwrap();
        assert_eq!(
            format!("{} {}", backend.name, backend.version),
            mode.backend()
        );
        assert_eq!(entry.modes, [mode.to_string()]);
        assert_eq!(entry.byte_orders, ["little"]);
        let instruction = decode_one(bytes, VirtualAddress::new(0), mode, Endian::Little).unwrap();
        assert_eq!(instruction.status, DecodeStatus::Decoded, "{id}");
        let unsupported = decode_one(bytes, VirtualAddress::new(0), mode, Endian::Big).unwrap();
        assert_eq!(unsupported.status, DecodeStatus::Unsupported, "{id}");
        assert!(!unsupported.bytes().is_empty());
        assert!(!entry.limitations.is_empty());
    }
}

#[test]
fn x86_assembly_capabilities_link_to_working_portable_modes() {
    for (id, mode) in [
        ("assemble.x86-16", DecodeMode::X86_16),
        ("assemble.x86-32", DecodeMode::X86_32),
        ("assemble.x86-64", DecodeMode::X86_64),
    ] {
        let report = current();
        let entry = report.require(id).unwrap();
        assert_eq!(entry.category, Category::Assembly);
        assert_eq!(entry.status, Status::Limited);
        assert_eq!(entry.modes, [mode.to_string()]);
        assert_eq!(entry.byte_orders, ["little"]);
        let choices = crate::assembly::assemble_x86(
            "nop",
            mode,
            Endian::Little,
            VirtualAddress::new(0),
            |_| None,
        )
        .unwrap();
        assert_eq!(choices[0], [0x90]);
    }
}

#[test]
fn optional_backends_and_research_assembly_remain_explicitly_unavailable() {
    let report = current();
    for id in [
        "assemble.arm",
        "assemble.thumb",
        "assemble.aarch64",
        "decode.mips32",
        "decode.mips64",
        "decode.powerpc32",
        "decode.powerpc64",
        "decode.riscv32",
        "decode.riscv64",
        "extensions.portable",
        "devices.raw",
    ] {
        let entry = report.get(id).unwrap();
        assert_eq!(entry.status, Status::Unavailable, "{id}");
        assert!(entry.backend.is_none());
        assert!(entry.modes.is_empty() && entry.byte_orders.is_empty());
        assert_eq!(report.require(id), Err(LookupError::Unavailable));
    }
    assert_eq!(
        report.require("decode.not-registered"),
        Err(LookupError::Absent)
    );
    assert!(report.get("decode.not-registered").is_none());
    assert_eq!(report.require("format.ne"), Err(LookupError::Unavailable));
    assert!(report.require("raw.read").is_ok());
    assert!(report.require("raw.overwrite").is_ok());
}

#[test]
fn registration_rejects_replacement_and_invalid_claims_without_changing_peers() {
    let mut registry = Registry::new();
    let raw = *current().get("raw.read").unwrap();
    registry.register(raw).unwrap();
    let mut replacement = raw;
    replacement.status = Status::Unavailable;
    replacement.limitations = &["Backend did not load"];
    assert_eq!(
        registry.register(replacement),
        Err(RegistrationError::Duplicate)
    );
    for invalid in [
        Capability { id: "", ..raw },
        Capability {
            id: "raw.\x1Bname",
            ..raw
        },
        Capability {
            id: "raw..name",
            ..raw
        },
        Capability {
            backend: None,
            id: "raw.no-backend",
            ..raw
        },
        Capability {
            status: Status::Limited,
            id: "raw.no-scope",
            ..raw
        },
        Capability {
            evidence: &[],
            id: "raw.no-evidence",
            ..raw
        },
        Capability {
            id: "raw.invalid-order",
            byte_orders: &["little"],
            ..replacement
        },
    ] {
        assert_eq!(
            registry.register(invalid),
            Err(RegistrationError::InvalidDescriptor)
        );
    }
    assert_eq!(registry.report().capabilities, &[raw]);
    assert_eq!(current().require("raw.read").unwrap(), &raw);
}

#[test]
fn registry_capacity_and_metadata_limits_are_bounded() {
    let names: Vec<_> = (0..=MAX_CAPABILITIES)
        .map(|index| format!("fixture.item-{index}"))
        .collect();
    let template = *current().get("raw.read").unwrap();
    let mut registry = Registry::new();
    let long_text = "x".repeat(MAX_TEXT_BYTES + 1);
    let long_items = [long_text.as_str()];
    assert_eq!(
        registry.register(Capability {
            limitations: &long_items,
            ..template
        }),
        Err(RegistrationError::InvalidDescriptor)
    );
    for name in &names[..MAX_CAPABILITIES] {
        registry
            .register(Capability {
                id: name,
                ..template
            })
            .unwrap();
    }
    assert_eq!(
        registry.register(Capability {
            id: &names[MAX_CAPABILITIES],
            ..template
        }),
        Err(RegistrationError::Capacity)
    );
    assert_eq!(registry.report().capabilities.len(), MAX_CAPABILITIES);
}

#[test]
fn unavailable_instruction_backend_preserves_raw_editing_and_format_inspection() {
    let mut bytes = crate::symbol_fixtures::pe64();
    bytes[0x84..0x86].copy_from_slice(&0x7777_u16.to_le_bytes());
    let mut editor = Editor::new(Document::new(MemorySource::new(bytes.clone())), false);
    editor.refresh_analysis();
    assert!(editor.executable().is_some());
    assert!(editor.decode_context().mode.is_none());
    assert_eq!(
        current().require("decode.mips64"),
        Err(LookupError::Unavailable)
    );
    assert!(current().require("format.pe").is_ok());
    editor
        .dispatch(Command::Goto(FileOffset::new(0x200)))
        .unwrap();
    editor.dispatch(Command::BeginEdit).unwrap();
    editor.dispatch(Command::WriteByte(0xA5)).unwrap();
    let mut actual = vec![0; bytes.len()];
    editor
        .document_mut()
        .read_at(FileOffset::new(0), &mut actual)
        .unwrap();
    bytes[0x200] = 0xA5;
    assert_eq!(actual, bytes);
    editor.dispatch(Command::Undo).unwrap();
    assert!(!editor.document().is_dirty());
}
