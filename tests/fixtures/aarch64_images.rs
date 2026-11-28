//! Synthetic ELF64 data in either byte order with a little-endian A64 code range.

pub const ARM64_BASE: u64 = 0x0001_4000_2000;
pub const ARM64_CODE_OFFSET: u64 = 0x100;

pub fn arm64_elf(big_endian: bool) -> Vec<u8> {
    let mut bytes = vec![0; ARM64_CODE_OFFSET as usize + 0x40];
    bytes[..7].copy_from_slice(&[b'\x7f', b'E', b'L', b'F', 2, 1 + u8::from(big_endian), 1]);
    for (at, value) in [(16, 2_u16), (18, 183), (52, 64), (54, 56), (56, 1)] {
        let field = if big_endian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        };
        bytes[at..at + 2].copy_from_slice(&field);
    }
    for (at, value) in [(20, 1_u32), (64, 1), (68, 5)] {
        let field = if big_endian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        };
        bytes[at..at + 4].copy_from_slice(&field);
    }
    for (at, value) in [
        (24, ARM64_BASE),        // entry
        (32, 64),                // ELF64 program table
        (72, ARM64_CODE_OFFSET), // PT_LOAD file offset
        (80, ARM64_BASE),        // virtual address
        (88, ARM64_BASE),        // physical address
        (96, 0x40),              // file size
        (104, 0x80),             // includes a zero-fill tail
        (112, 1),                // alignment
    ] {
        let field = if big_endian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        };
        bytes[at..at + 8].copy_from_slice(&field);
    }
    // Independent LLVM MC 23.1.6 encodings for the AArch64 GNU target.
    for (index, word) in [
        0x1000_0100_u32, // adr x0, #32
        0x9000_0001,     // adrp x1, #0
        0x5800_0143,     // ldr x3, #40
        0xd800_0220,     // prfm pldl1keep, #68 (zero-fill target)
        0x1400_0002,     // b #8
        0xd503_201f,     // nop
        0xd65f_03c0,     // ret
    ]
    .into_iter()
    .enumerate()
    {
        let at = ARM64_CODE_OFFSET as usize + index * 4;
        bytes[at..at + 4].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}
