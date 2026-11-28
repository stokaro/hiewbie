use super::*;

fn word_bytes(text: &str) -> [u8; 4] {
    u32::from_str_radix(text, 16).unwrap().to_be_bytes()
}

#[test]
fn aarch64_versioned_baseline_and_extension_vectors() {
    for row in include_str!("../../../../tests/fixtures/aarch64-decode.tsv").lines() {
        if row.starts_with('#') {
            continue;
        }
        let [family, bytes, source, expected]: [&str; 4] =
            row.split('\t').collect::<Vec<_>>().try_into().unwrap();
        let bytes = word_bytes(bytes);
        let decoded = decode_one(
            &bytes,
            VirtualAddress::new(0x1000),
            DecodeMode::Aarch64,
            Endian::Little,
        )
        .unwrap();
        assert_eq!(decoded.status, DecodeStatus::Decoded, "{family}: {source}");
        assert_eq!(decoded.bytes(), bytes, "{family}: {source}");
        assert_eq!(decoded.text, expected, "{family}: {source}");
        assert_eq!(decoded.direct_branch, None);
        assert_eq!(decoded.address_operand, None);
    }
}

#[test]
fn aarch64_branch_and_pc_relative_data_use_checked_architectural_addresses() {
    for row in include_str!("../../../../tests/fixtures/aarch64-pc-relative.tsv").lines() {
        if row.starts_with('#') {
            continue;
        }
        let [pc, target, kind, bytes, source]: [&str; 5] =
            row.split('\t').collect::<Vec<_>>().try_into().unwrap();
        let address = VirtualAddress::new(u64::from_str_radix(pc, 16).unwrap());
        let target_address =
            (target != "-").then(|| VirtualAddress::new(u64::from_str_radix(target, 16).unwrap()));
        let bytes = word_bytes(bytes);
        let mode = DecodeMode::Aarch64;
        let decoded = decode_one(&bytes, address, mode, Endian::Little).unwrap();
        assert_eq!(decoded.status, DecodeStatus::Decoded, "{row}");
        assert_eq!(decoded.bytes(), bytes, "{row}");
        assert_eq!(
            decoded.direct_branch,
            target_address
                .filter(|_| kind == "branch")
                .map(|address| BranchTarget { address, mode }),
            "{row}"
        );
        assert_eq!(
            decoded.address_operand,
            target_address.and_then(|address| match kind {
                "address" => Some(AddressOperand::Literal(address)),
                "memory" => Some(AddressOperand::Memory(address)),
                _ => None,
            }),
            "{row}"
        );
        let rendered_address = target_address.map_or_else(
            || "<address out of range>".to_owned(),
            |address| format!("0x{:x}", address.get()),
        );
        assert!(
            decoded
                .text
                .starts_with(source.split_whitespace().next().unwrap())
                && decoded.text.ends_with(&rendered_address),
            "{row}: {}",
            decoded.text
        );
    }
}

#[test]
fn aarch64_unknown_unsupported_and_truncated_encodings_preserve_raw_bytes() {
    for (bytes, status, reason) in [
        (
            &[0x20, 0x00, 0xa2, 0x65][..],
            DecodeStatus::Unsupported,
            "encoding class",
        ), // SVE FMLA
        (
            &[0x60, 0x20, 0x84, 0x80],
            DecodeStatus::Unsupported,
            "encoding class",
        ), // SME FMOPA
        (
            &[0x00, 0x40, 0x20, 0x45],
            DecodeStatus::Unsupported,
            "encoding class",
        ), // reserved SQXTN width
        (
            &[0xff, 0xff, 0xff, 0xff],
            DecodeStatus::Unknown,
            "unknown to backend",
        ),
        (
            &[0xe0, 0xe3, 0x98],
            DecodeStatus::Truncated,
            "truncated instruction",
        ),
    ] {
        let row = decode_one(
            bytes,
            VirtualAddress::new(0),
            DecodeMode::Aarch64,
            Endian::Little,
        )
        .unwrap();
        assert_eq!(row.status, status, "{bytes:02x?}: {}", row.text);
        assert_eq!(row.bytes(), bytes);
        assert!(row.text.contains(reason));
        assert_eq!(row.direct_branch, None);
        assert_eq!(row.address_operand, None);
    }
}
