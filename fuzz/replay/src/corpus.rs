//! Original MIT-licensed generated fixtures. No executable samples are imported.
//! The PE fixture is shared with the existing frontend symbol tests.

#[path = "../../../tests/fixtures/symbol_images.rs"]
mod symbol_fixture;

use crate::Target;

pub struct Seed {
    pub target: Target,
    pub name: String,
    pub bytes: Vec<u8>,
}

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn elf() -> Vec<u8> {
    let mut bytes = vec![0; 0x600];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    put16(&mut bytes, 16, 2);
    put16(&mut bytes, 18, 62);
    put32(&mut bytes, 20, 1);
    put64(&mut bytes, 24, 0x40_0200);
    put64(&mut bytes, 32, 64);
    put64(&mut bytes, 40, 0x400);
    put16(&mut bytes, 52, 64);
    put16(&mut bytes, 54, 56);
    put16(&mut bytes, 56, 1);
    put16(&mut bytes, 58, 64);
    put16(&mut bytes, 60, 4);
    put32(&mut bytes, 64, 1);
    put32(&mut bytes, 68, 5);
    put64(&mut bytes, 80, 0x40_0000);
    put64(&mut bytes, 96, 0x600);
    put64(&mut bytes, 104, 0x700);
    put64(&mut bytes, 112, 0x1000);
    put32(&mut bytes, 0x444, 1);
    put64(&mut bytes, 0x448, 6);
    put64(&mut bytes, 0x450, 0x40_0200);
    put64(&mut bytes, 0x458, 0x200);
    put64(&mut bytes, 0x460, 16);
    put32(&mut bytes, 0x484, 2);
    put64(&mut bytes, 0x498, 0x300);
    put64(&mut bytes, 0x4a0, 48);
    put32(&mut bytes, 0x4a8, 3);
    put32(&mut bytes, 0x4ac, 1);
    put64(&mut bytes, 0x4b0, 8);
    put64(&mut bytes, 0x4b8, 24);
    put32(&mut bytes, 0x4c4, 3);
    put64(&mut bytes, 0x4d8, 0x350);
    put64(&mut bytes, 0x4e0, 7);
    put32(&mut bytes, 0x318, 1);
    bytes[0x31c] = 0x12;
    put16(&mut bytes, 0x31e, 1);
    put64(&mut bytes, 0x320, 0x40_0200);
    put64(&mut bytes, 0x328, 1);
    bytes[0x350..0x357].copy_from_slice(b"\0entry\0");
    bytes
}

fn macho() -> Vec<u8> {
    let mut bytes = vec![0; 0x400];
    put32(&mut bytes, 0, 0xfeed_facf);
    put32(&mut bytes, 4, 0x0100_0007);
    put32(&mut bytes, 12, 2);
    put32(&mut bytes, 16, 2);
    put32(&mut bytes, 20, 176);
    put32(&mut bytes, 32, 0x19);
    put32(&mut bytes, 36, 152);
    bytes[40..46].copy_from_slice(b"__TEXT");
    put64(&mut bytes, 56, 0x1_0000_0000);
    put64(&mut bytes, 64, 0x400);
    put64(&mut bytes, 80, 0x400);
    put32(&mut bytes, 88, 7);
    put32(&mut bytes, 92, 5);
    put32(&mut bytes, 96, 1);
    bytes[104..110].copy_from_slice(b"__text");
    bytes[120..126].copy_from_slice(b"__TEXT");
    put64(&mut bytes, 136, 0x1_0000_0200);
    put64(&mut bytes, 144, 16);
    put32(&mut bytes, 152, 0x200);
    put32(&mut bytes, 184, 2);
    put32(&mut bytes, 188, 24);
    put32(&mut bytes, 192, 0x300);
    put32(&mut bytes, 196, 1);
    put32(&mut bytes, 200, 0x310);
    put32(&mut bytes, 204, 8);
    put32(&mut bytes, 0x300, 1);
    bytes[0x304] = 0x0f;
    bytes[0x305] = 1;
    put64(&mut bytes, 0x308, 0x1_0000_0200);
    bytes[0x310..0x318].copy_from_slice(b"\0_entry\0");
    bytes
}

fn fat_macho() -> Vec<u8> {
    let image = macho();
    let mut bytes = vec![0; 0x100 + image.len()];
    for (at, value) in [
        (0, 0xcafe_babe_u32),
        (4, 1),
        (8, 0x0100_0007),
        (12, 3),
        (16, 0x100),
        (20, image.len() as u32),
        (24, 8),
    ] {
        bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }
    bytes[0x100..].copy_from_slice(&image);
    bytes
}

/// Named, reproducible initial corpora. Retain future minimized regressions
/// here with a comment linking their defect and input provenance.
pub fn seeds() -> Vec<Seed> {
    let mut seeds = Vec::new();
    let mut add = |target, name: &str, bytes| {
        seeds.push(Seed {
            target,
            name: name.into(),
            bytes,
        })
    };
    for target in Target::ALL {
        add(target, "empty", Vec::new());
        add(target, "zeroes", vec![0; 64]);
        add(target, "ones", vec![0xff; 96]);
    }
    for (name, bytes) in [
        ("pe64-symbols", symbol_fixture::pe64()),
        ("elf64-symbols", elf()),
        ("macho64-symbols", macho()),
        ("fat-macho", fat_macho()),
    ] {
        add(Target::FormatsSymbols, name, bytes);
    }
    let mut huge = symbol_fixture::pe64();
    huge[0x86..0x88].fill(0xff);
    huge[0x3c..0x40].fill(0xff);
    add(Target::FormatsSymbols, "huge-pe-count-and-offset", huge);
    let mut lossy = symbol_fixture::pe64();
    put32(&mut lossy, 0x450, 0x1400);
    lossy[0x600..0x790].fill(0xff);
    lossy[0x790] = 0;
    add(Target::FormatsSymbols, "lossy-export-name", lossy);
    let instructions = [
        0x90, 0xeb, 0xfe, 0x66, 0x0f, 0x0b, 0xff, 0xc3, 0x00, 0x00, 0xa0, 0xe1, 0x00, 0xbf, 0x1f,
        0x20, 0x03, 0xd5,
    ];
    for mode in 0..6 {
        let mut input = vec![mode, 0];
        input.extend_from_slice(&(if mode == 2 { u64::MAX - 4 } else { 0x1000 }).to_le_bytes());
        input.extend_from_slice(&instructions);
        add(Target::Decoder, &format!("mode-{mode}"), input);
    }
    let mut mapping = vec![0; 49 + 64];
    for (at, value) in [
        (0, 0x1000),
        (8, 16),
        (16, 32),
        (24, 18),
        (32, 0x1002),
        (40, 2),
        (49, 16),
        (57, 8),
        (65, 0x1000),
        (73, 16),
    ] {
        put64(&mut mapping, at, value);
    }
    add(Target::Addresses, "mapped-zero-fill", mapping.clone());
    mapping.copy_within(49..81, 81);
    add(Target::Addresses, "overlapping-mappings", mapping);
    for flags in [0_u8, 1, 2, 3, 4, 5, 8, 9, 32, 33] {
        let mut input = vec![flags, 3];
        input.extend_from_slice(&(if flags & 1 != 0 { 8_u64 } else { 0 }).to_le_bytes());
        input.extend_from_slice(&[1, 0, 9, 0]);
        input.extend_from_slice(b"aBaABabaBaBa");
        add(Target::Search, &format!("flags-{flags}"), input);
    }
    let operations = [
        0, 0, 0, 2, b'X', 0, 2, b'Y', 0, 4, 0, 0, 12, 0, 0, 13, 0, 0, 5, 0, 0, 6, 0, 0, 3, 0, 0, 8,
        0, 0, 9, 0, 0, 10, 0, 0, 1, 0, 0, 0, 0, 0, 2, b'Z', 0, 9, 1, 0,
    ];
    for read_only in [0, 1] {
        let mut input = vec![read_only, 4];
        input.extend_from_slice(b"abcd");
        input.extend_from_slice(&operations);
        add(
            Target::EditCommands,
            &format!("transaction-readonly-{read_only}"),
            input,
        );
    }
    let mut input = vec![0, 0];
    input.extend_from_slice(&operations);
    add(Target::EditCommands, "empty-edit-transaction", input);
    seeds
}

#[cfg(test)]
mod tests {
    use hiewbie_core::{Document, MemorySource, formats, symbols};

    #[test]
    fn generated_parser_seeds_reach_each_symbol_backend() {
        for bytes in [super::symbol_fixture::pe64(), super::elf(), super::macho()] {
            let mut document = Document::new(MemorySource::new(bytes));
            let image = formats::inspect(&mut document, None).unwrap().unwrap();
            let symbols = symbols::inspect(&mut document, &image).unwrap();
            assert!(!symbols.entries.is_empty());
        }
    }

    #[test]
    fn lossy_seed_reaches_an_expanded_symbol_name() {
        let seed = super::seeds()
            .into_iter()
            .find(|seed| seed.name == "lossy-export-name")
            .unwrap();
        let mut document = Document::new(MemorySource::new(seed.bytes));
        let image = formats::inspect(&mut document, None).unwrap().unwrap();
        let symbols = symbols::inspect(&mut document, &image).unwrap();
        let expected = "\u{fffd}".repeat(400);
        assert!(expected.len() > symbols::MAX_SYMBOL_NAME_BYTES);
        assert!(symbols.entries.iter().any(|symbol| symbol.name == expected));
    }
}
