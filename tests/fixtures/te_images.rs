//! Original MIT-licensed TE images built from public UEFI PI / PE field layouts.
//! No executable, firmware, PDB, or proprietary HIEW bytes are redistributed.

pub const STRIPPED_SIZE: u16 = 0x120;
pub const BIAS: usize = STRIPPED_SIZE as usize - 40;
pub const ENTRY: usize = 0x1010 - BIAS;
pub const RELOCATION_TARGET: usize = 0x1020 - BIAS;
pub const RELOCATIONS: usize = 0x2000 - BIAS;
pub const DEBUG: usize = 0x2040 - BIAS;
pub const DEBUG_DATA: usize = 0x20c0 - BIAS;
pub const BASE32: u64 = 0x40_0000;
pub const BASE64: u64 = 0x1_4000_0000;

pub fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

pub fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

pub fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

pub fn te32() -> Vec<u8> {
    te(0x14c)
}

pub fn te64() -> Vec<u8> {
    te(0x8664)
}

/// UEFI ARMTHUMB_MIXED with an explicitly Thumb entry in an ARM-family image.
pub fn te_mixed_thumb() -> Vec<u8> {
    let mut bytes = te(0x1c2);
    put32(&mut bytes, 8, 0x1011);
    bytes[ENTRY..ENTRY + 4].copy_from_slice(&[0x00, 0xbf, 0x70, 0x47]);
    bytes
}

/// Same 40-byte header for every machine, with two sections, one known and one
/// unsupported relocation, a CodeView record, and an unknown debug record.
pub fn te(machine: u16) -> Vec<u8> {
    let wide = matches!(machine, 0x8664 | 0xaa64);
    let mut bytes = vec![0; 0x2200 - BIAS];
    bytes[..2].copy_from_slice(b"VZ");
    put16(&mut bytes, 2, machine);
    bytes[4] = 2;
    bytes[5] = 10;
    put16(&mut bytes, 6, STRIPPED_SIZE);
    put32(
        &mut bytes,
        8,
        if machine == 0x1c4 { 0x1011 } else { 0x1010 },
    );
    put32(&mut bytes, 12, 0x1000);
    put64(&mut bytes, 16, if wide { BASE64 } else { BASE32 });
    put32(&mut bytes, 24, 0x2000);
    put32(&mut bytes, 28, 12);
    put32(&mut bytes, 32, 0x2040);
    put32(&mut bytes, 36, 56);
    bytes[40..45].copy_from_slice(b".text");
    put32(&mut bytes, 48, 0x100);
    put32(&mut bytes, 52, 0x1000);
    put32(&mut bytes, 56, 0x100);
    put32(&mut bytes, 60, 0x1000);
    put32(&mut bytes, 76, 0x6000_0020);
    bytes[80..85].copy_from_slice(b".data");
    put32(&mut bytes, 88, 0x240);
    put32(&mut bytes, 92, 0x2000);
    put32(&mut bytes, 96, 0x200);
    put32(&mut bytes, 100, 0x2000);
    put32(&mut bytes, 116, 0xc000_0040);
    match machine {
        0x1c0 | 0x1c2 => bytes[ENTRY..ENTRY + 4].copy_from_slice(&[0x00, 0x00, 0xa0, 0xe1]),
        0x1c4 => bytes[ENTRY..ENTRY + 2].copy_from_slice(&[0x00, 0xbf]),
        0xaa64 => bytes[ENTRY..ENTRY + 4].copy_from_slice(&[0x1f, 0x20, 0x03, 0xd5]),
        _ => bytes[ENTRY..ENTRY + 2].copy_from_slice(&[0x90, 0xc3]),
    }
    put64(
        &mut bytes,
        RELOCATION_TARGET,
        if wide {
            BASE64 + 0x1010
        } else {
            BASE32 + 0x1010
        },
    );
    if machine == 0x1c0 {
        // ARM MOVW r0,#0x1010; MOVT r0,#0x40 (address 0x401010).
        put32(&mut bytes, RELOCATION_TARGET, 0xe301_0010);
        put32(&mut bytes, RELOCATION_TARGET + 4, 0xe340_0040);
    } else if matches!(machine, 0x1c2 | 0x1c4) {
        // Thumb-2 MOVW/MOVT with the same original address.
        bytes[RELOCATION_TARGET..RELOCATION_TARGET + 8]
            .copy_from_slice(&[0x41, 0xf2, 0x10, 0x00, 0xc0, 0xf2, 0x40, 0x00]);
    }
    put32(&mut bytes, RELOCATIONS, 0x1000);
    put32(&mut bytes, RELOCATIONS + 4, 12);
    let relocation = match machine {
        0x1c0 => 5,
        0x1c2 | 0x1c4 => 7,
        _ if wide => 10,
        _ => 3,
    };
    put16(&mut bytes, RELOCATIONS + 8, (relocation << 12) | 0x20);
    put16(&mut bytes, RELOCATIONS + 10, 0xf030);
    put32(&mut bytes, DEBUG + 4, 0x1234_5678);
    put32(&mut bytes, DEBUG + 12, 2);
    put32(&mut bytes, DEBUG + 16, 40);
    put32(&mut bytes, DEBUG + 20, 0x20c0);
    put32(&mut bytes, DEBUG + 24, 0x20c0);
    put32(&mut bytes, DEBUG + 28 + 12, 0xfeed);
    put32(&mut bytes, DEBUG + 28 + 16, 4);
    put32(&mut bytes, DEBUG + 28 + 20, 0x2100);
    put32(&mut bytes, DEBUG + 28 + 24, 0x2100);
    bytes[DEBUG_DATA..DEBUG_DATA + 4].copy_from_slice(b"RSDS");
    for index in 0..16 {
        bytes[DEBUG_DATA + 4 + index] = index as u8;
    }
    put32(&mut bytes, DEBUG_DATA + 20, 1);
    bytes[DEBUG_DATA + 24..DEBUG_DATA + 37].copy_from_slice(b"original.pdb\0");
    bytes[0x2100 - BIAS..0x2104 - BIAS].copy_from_slice(b"RAW!");
    bytes
}
