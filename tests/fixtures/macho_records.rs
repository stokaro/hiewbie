//! Original MIT-licensed raw load-command fixtures, independent of parser code.

pub fn unknown_commands(count: u32) -> Vec<u8> {
    let mut bytes = vec![0; 28 + count as usize * 8];
    for (offset, value) in [
        (0, 0xfeed_face),
        (4, 7),
        (8, 3),
        (12, 1),
        (16, count),
        (20, count * 8),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for index in 0..count as usize {
        let at = 28 + index * 8;
        bytes[at..at + 4].copy_from_slice(&(0x7000_0000 + index as u32).to_le_bytes());
        bytes[at + 4..at + 8].copy_from_slice(&8_u32.to_le_bytes());
    }
    bytes
}

pub fn fat_records() -> Vec<u8> {
    let thin = unknown_commands(2);
    let mut bytes = vec![0; 0x300];
    for (at, value) in [
        (0, 0xcafe_babe_u32),
        (4, 2),
        (8, 7),
        (12, 3),
        (16, 0x100),
        (20, 0x100),
        (24, 8),
        (28, 7),
        (32, 3),
        (36, 0x200),
        (40, 0x100),
        (44, 8),
    ] {
        bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }
    bytes[0x100..0x100 + thin.len()].copy_from_slice(&thin);
    bytes[0x200..0x200 + thin.len()].copy_from_slice(&thin);
    bytes
}
