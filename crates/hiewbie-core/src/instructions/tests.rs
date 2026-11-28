use super::*;

fn decode(bytes: &[u8], mode: DecodeMode, address: u64) -> DecodedInstruction {
    decode_one(bytes, VirtualAddress::new(address), mode, Endian::Little).unwrap()
}

fn branch(bytes: &[u8], mode: DecodeMode, address: u64, expected: u64, next_mode: DecodeMode) {
    let decoded = decode(bytes, mode, address);
    assert_eq!(decoded.status, DecodeStatus::Decoded, "{}", decoded.text);
    assert_eq!(decoded.bytes(), bytes);
    assert_eq!(
        decoded.direct_branch,
        Some(BranchTarget {
            address: VirtualAddress::new(expected),
            mode: next_mode
        }),
        "{}",
        decoded.text,
    );
    assert!(!decoded.text.contains('$'), "{}", decoded.text);
}

#[test]
fn x86_modes_have_independent_instruction_lengths() {
    let bytes = [0xb8, 0x34, 0x12, 0x78, 0x56];
    let short = decode(&bytes, DecodeMode::X86_16, 0x1000);
    let wide = decode(&bytes, DecodeMode::X86_32, 0x1000);
    assert_eq!(short.length, 3);
    assert_eq!(wide.length, 5);
    assert_eq!(short.bytes(), &bytes[..3]);
    assert!(short.text.contains("ax"));
    assert!(wide.text.contains("eax"));
    let long = decode(&[0x48, 0x89, 0xe5], DecodeMode::X86_64, 0x1000);
    assert_eq!(long.status, DecodeStatus::Decoded);
    assert_eq!(long.length, 3);
    assert!(long.text.contains("rbp"));
}

#[test]
fn x86_avx_is_not_disabled_by_feature_selection() {
    let decoded = decode(&[0xc5, 0xf8, 0x77], DecodeMode::X86_64, 0);
    assert_eq!(decoded.status, DecodeStatus::Decoded);
    assert_eq!(decoded.text, "vzeroupper");
}

#[test]
fn x86_extension_corpus_preserves_length_operands_and_bounded_truncation() {
    for entry in include_str!("../../../../tests/fixtures/x86-decode-8.63.tsv").lines() {
        if entry.starts_with('#') || entry.is_empty() {
            continue;
        }
        let fields: Vec<_> = entry.split('\t').collect();
        let [bits, offset, family, hex, expected, _assembly] = fields.as_slice() else {
            panic!("malformed fixture row: {entry}");
        };
        let mode = match *bits {
            "16" => DecodeMode::X86_16,
            "32" => DecodeMode::X86_32,
            "64" => DecodeMode::X86_64,
            _ => panic!("unexpected fixture bitness: {bits}"),
        };
        let address = u64::from_str_radix(offset, 16).unwrap();
        let bytes: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
            .collect();
        let decoded = decode(&bytes, mode, address);
        assert_eq!(decoded.status, DecodeStatus::Decoded, "{family}: {entry}");
        assert_eq!(decoded.bytes(), bytes, "{entry}");
        assert_eq!(decoded.text, *expected, "{entry}");
        assert_eq!(decoded.direct_branch, None, "{entry}");

        if bytes.len() > 1 {
            let tail = decode(&bytes[..bytes.len() - 1], mode, address);
            assert_eq!(tail.status, DecodeStatus::Truncated, "{entry}");
            assert_eq!(tail.bytes(), &bytes[..1], "{entry}");
            assert_eq!(tail.direct_branch, None, "{entry}");
        }
    }
}

#[test]
fn x86_direct_targets_use_actual_instruction_addresses() {
    branch(
        &[0xe8, 0xfb, 0xff, 0xff, 0xff],
        DecodeMode::X86_32,
        0x1000,
        0x1000,
        DecodeMode::X86_32,
    );
    branch(
        &[0x75, 0x0e],
        DecodeMode::X86_64,
        0x1000,
        0x1010,
        DecodeMode::X86_64,
    );
    branch(
        &[0xe2, 0xfe],
        DecodeMode::X86_16,
        0x1000,
        0x1000,
        DecodeMode::X86_16,
    );
}

#[test]
fn x86_decoder_crosses_four_gib_without_underflow() {
    // iced-x86 1.22.0 fixes an upstream debug/overflow-check decoder panic here.
    branch(
        &[0xeb, 0],
        DecodeMode::X86_64,
        0xffff_ffff,
        0x1_0000_0001,
        DecodeMode::X86_64,
    );
    let decoded = decode(&[0x48, 0x89, 0xe5], DecodeMode::X86_64, 0xffff_fffe);
    assert_eq!(decoded.status, DecodeStatus::Decoded);
    assert_eq!(decoded.address.get(), 0xffff_fffe);
}

#[test]
fn x86_near_branches_retain_architectural_address_width() {
    branch(
        &[0xeb, 0],
        DecodeMode::X86_16,
        0xffff,
        1,
        DecodeMode::X86_16,
    );
    branch(
        &[0xeb, 0],
        DecodeMode::X86_32,
        0xffff_ffff,
        1,
        DecodeMode::X86_32,
    );
    branch(
        &[0xeb, 0],
        DecodeMode::X86_64,
        u64::MAX - 1,
        0,
        DecodeMode::X86_64,
    );
}

#[test]
fn indirect_and_far_x86_branches_are_not_invented() {
    let indirect = decode(&[0xff, 0xe0], DecodeMode::X86_64, 0x1000);
    assert_eq!(indirect.status, DecodeStatus::Decoded);
    assert_eq!(indirect.direct_branch, None);
    let far = decode(&[0xea, 0x34, 0x12, 0x78, 0x56], DecodeMode::X86_16, 0x1000);
    assert_eq!(far.status, DecodeStatus::Decoded);
    assert_eq!(far.direct_branch, None);
}

#[test]
fn unknown_and_truncated_x86_keep_raw_bytes_and_progress() {
    let unknown = decode(&[0x66; 15], DecodeMode::X86_64, 0);
    assert_eq!(unknown.status, DecodeStatus::Unknown);
    assert_eq!(unknown.bytes(), &[0x66]);
    let tail = decode(&[0x0f], DecodeMode::X86_64, 0);
    assert_eq!(tail.status, DecodeStatus::Truncated);
    assert_eq!(tail.bytes(), &[0x0f]);
}

#[test]
fn complete_legacy_only_opcodes_are_distinct_from_unknown_in_long_mode() {
    for (bytes, mnemonic) in [
        (&[0x60][..], "pusha"),
        (&[0xd4, 0x0a][..], "aam 0xa"),
        (&[0xce][..], "into"),
    ] {
        let legacy = decode(bytes, DecodeMode::X86_16, 0);
        assert_eq!(legacy.status, DecodeStatus::Decoded);
        assert_eq!(legacy.text, mnemonic);
        let long = decode(bytes, DecodeMode::X86_64, 0);
        assert_eq!(long.status, DecodeStatus::InvalidForMode);
        assert_eq!(long.bytes(), &bytes[..1]);
        assert_eq!(long.direct_branch, None);
    }
    let shared = decode(&[0x62, 0, 0, 0, 0, 0], DecodeMode::X86_64, 0);
    assert_ne!(shared.status, DecodeStatus::InvalidForMode);
    let missing_base = decode(&[0xd4], DecodeMode::X86_64, 0);
    assert_ne!(missing_base.status, DecodeStatus::InvalidForMode);
}

#[test]
fn arm_and_thumb_have_distinct_lengths() {
    let arm = decode(&[0, 0, 0xa0, 0xe1], DecodeMode::Arm, 0);
    assert_eq!(arm.status, DecodeStatus::Decoded);
    assert_eq!(arm.text, "mov r0, r0");
    assert_eq!(arm.length, 4);
    let thumb = decode(&[0, 0xbf], DecodeMode::Thumb, 0);
    assert_eq!(thumb.status, DecodeStatus::Decoded);
    assert_eq!(thumb.text, "nop");
    assert_eq!(thumb.length, 2);
    let wide = decode(&[0, 0xf0, 0, 0xf8], DecodeMode::Thumb, 0);
    assert_eq!(wide.status, DecodeStatus::Decoded);
    assert_eq!(wide.length, 4);
}

#[test]
fn arm_branches_include_pc_bias_and_mode_exchange() {
    branch(
        &[0, 0, 0, 0xea],
        DecodeMode::Arm,
        0x1000,
        0x1008,
        DecodeMode::Arm,
    );
    branch(
        &[0xfe, 0xff, 0xff, 0xeb],
        DecodeMode::Arm,
        0x1000,
        0x1000,
        DecodeMode::Arm,
    );
    branch(
        &[0, 0, 0, 0xfa],
        DecodeMode::Arm,
        0x1000,
        0x1008,
        DecodeMode::Thumb,
    );
    branch(
        &[0, 0, 0, 0xfb],
        DecodeMode::Arm,
        0x1000,
        0x100a,
        DecodeMode::Thumb,
    );
}

#[test]
fn thumb_branches_apply_pc_bias_to_signed_and_unsigned_offsets() {
    branch(
        &[0xfe, 0xe7],
        DecodeMode::Thumb,
        0x1000,
        0x1000,
        DecodeMode::Thumb,
    );
    branch(
        &[0, 0xd0],
        DecodeMode::Thumb,
        0x1000,
        0x1004,
        DecodeMode::Thumb,
    );
    branch(
        &[0, 0xb1],
        DecodeMode::Thumb,
        0x1000,
        0x1004,
        DecodeMode::Thumb,
    );
    branch(
        &[0, 0xf0, 0, 0xf8],
        DecodeMode::Thumb,
        0x1000,
        0x1004,
        DecodeMode::Thumb,
    );
    branch(
        &[0, 0xf0, 0, 0xb8],
        DecodeMode::Thumb,
        0x1000,
        0x1004,
        DecodeMode::Thumb,
    );
    branch(
        &[0, 0xf0, 0, 0x80],
        DecodeMode::Thumb,
        0x1000,
        0x1004,
        DecodeMode::Thumb,
    );
}

#[test]
fn thumb_blx_aligns_pc_before_exchanging_mode() {
    branch(
        &[0, 0xf0, 0, 0xe8],
        DecodeMode::Thumb,
        0x1002,
        0x1004,
        DecodeMode::Arm,
    );
}

#[test]
fn arm32_pc_wrapping_is_separate_from_64_bit_file_positions() {
    branch(
        &[0, 0, 0, 0xea],
        DecodeMode::Arm,
        0xffff_fffc,
        4,
        DecodeMode::Arm,
    );
    branch(
        &[0, 0xe0],
        DecodeMode::Thumb,
        0xffff_fffe,
        2,
        DecodeMode::Thumb,
    );
    branch(
        &[0, 0xf0, 0, 0xe8],
        DecodeMode::Thumb,
        0xffff_fffe,
        0,
        DecodeMode::Arm,
    );
    let high = decode(&[0, 0, 0, 0xea], DecodeMode::Arm, 0x1_0000_0000);
    assert_eq!(high.status, DecodeStatus::Decoded);
    assert_eq!(high.direct_branch, None);
    assert!(high.text.contains("out of range"));
}

#[test]
fn strict_arm_mode_rejects_upstream_permissive_panic_fixture() {
    // Independently generated word 0x4120ffd2 asks for a paired r15/r16 operand.
    // yaxpeax-arm's permissive setting panics constructing r16; strict operand
    // validation must return a nondecoded row without hiding backend panics.
    let decoded = decode(&[0xd2, 0xff, 0x20, 0x41], DecodeMode::Arm, 0);
    assert_eq!(decoded.status, DecodeStatus::Unknown);
    assert_eq!(decoded.bytes(), &[0xd2, 0xff, 0x20, 0x41]);
    assert!(decoded.direct_branch.is_none());
}

#[test]
fn aarch64_scalar_and_direct_branches_use_current_pc() {
    let nop = decode(&[0x1f, 0x20, 3, 0xd5], DecodeMode::Aarch64, 0);
    assert_eq!(nop.text, "nop");
    assert_eq!(nop.status, DecodeStatus::Decoded);
    branch(
        &[1, 0, 0, 0x14],
        DecodeMode::Aarch64,
        0x1000,
        0x1004,
        DecodeMode::Aarch64,
    );
    branch(
        &[0xff, 0xff, 0xff, 0x97],
        DecodeMode::Aarch64,
        0x1000,
        0x0ffc,
        DecodeMode::Aarch64,
    );
    branch(
        &[0x20, 0, 0, 0x54],
        DecodeMode::Aarch64,
        0x1000,
        0x1004,
        DecodeMode::Aarch64,
    );
    branch(
        &[0x20, 0, 0, 0xb4],
        DecodeMode::Aarch64,
        0x1000,
        0x1004,
        DecodeMode::Aarch64,
    );
    branch(
        &[0x20, 0, 0, 0x36],
        DecodeMode::Aarch64,
        0x1000,
        0x1004,
        DecodeMode::Aarch64,
    );
    let ret = decode(&[0xc0, 3, 0x5f, 0xd6], DecodeMode::Aarch64, 0x1000);
    assert_eq!(ret.text, "ret");
    assert_eq!(ret.direct_branch, None);
}

#[test]
fn aarch64_branch_address_overflow_does_not_wrap_to_file_bytes() {
    let high = decode(&[1, 0, 0, 0x14], DecodeMode::Aarch64, u64::MAX - 1);
    let low = decode(&[0xff, 0xff, 0xff, 0x17], DecodeMode::Aarch64, 0);
    for decoded in [high, low] {
        assert_eq!(decoded.status, DecodeStatus::Decoded);
        assert_eq!(decoded.direct_branch, None);
        assert!(decoded.text.contains("out of range"));
    }
}

#[test]
fn unsupported_instruction_endianness_is_visible() {
    for mode in [
        DecodeMode::X86_16,
        DecodeMode::X86_32,
        DecodeMode::X86_64,
        DecodeMode::Arm,
        DecodeMode::Thumb,
        DecodeMode::Aarch64,
    ] {
        let decoded = decode_one(&[0, 0, 0, 0], VirtualAddress::new(0), mode, Endian::Big).unwrap();
        assert_eq!(decoded.status, DecodeStatus::Unsupported);
        assert_eq!(decoded.length, mode.alignment());
        assert!(decoded.direct_branch.is_none());
        assert!(decoded.text.contains("unsupported"));
    }
}

#[test]
fn instruction_input_and_output_budgets_are_independent() {
    let decoded = decode(&[0x90; 4096], DecodeMode::X86_64, 0);
    assert_eq!(decoded.bytes(), &[0x90]);
    let mut text = TextBuffer::new();
    for _ in 0..300 {
        text.append("é");
    }
    let text = text.finish();
    assert!(text.len() <= MAX_INSTRUCTION_TEXT_BYTES);
    assert!(text.ends_with("..."));
}

#[test]
fn mode_names_round_trip_without_implying_assembly() {
    for mode in [
        DecodeMode::X86_16,
        DecodeMode::X86_32,
        DecodeMode::X86_64,
        DecodeMode::Arm,
        DecodeMode::Thumb,
        DecodeMode::Aarch64,
    ] {
        assert_eq!(mode.to_string().parse(), Ok(mode));
        assert!(!mode.backend().is_empty());
    }
    assert_eq!("arm64".parse(), Ok(DecodeMode::Aarch64));
    assert!("mips".parse::<DecodeMode>().is_err());
}

#[test]
fn bounded_generated_corpus_preserves_progress_and_never_panics() {
    // A deterministic, reproducible malformed-input probe, not exhaustive ISA
    // coverage. Every architecture sees the same generated bytes and EOF tails.
    let modes = [
        DecodeMode::X86_16,
        DecodeMode::X86_32,
        DecodeMode::X86_64,
        DecodeMode::Arm,
        DecodeMode::Thumb,
        DecodeMode::Aarch64,
    ];
    let mut state = 0x600d_5eed_u64;
    for _ in 0..65_536 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bytes = (state as u32).to_le_bytes();
        for mode in modes {
            assert!(decode_one(&[], VirtualAddress::new(0), mode, Endian::Little).is_none());
            for length in 1..=bytes.len() {
                let decoded = decode(&bytes[..length], mode, 0x1000);
                assert!(decoded.length > 0 && decoded.length <= length);
                assert_eq!(decoded.bytes(), &bytes[..decoded.length]);
                assert!(decoded.text.len() <= MAX_INSTRUCTION_TEXT_BYTES);
            }
        }
    }
}
