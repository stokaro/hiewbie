//! Independently generated integer fields and names, shared by frontend tests.
//! This contains no external executable or proprietary HIEW material.

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

pub fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

pub fn pe64() -> Vec<u8> {
    let mut bytes = vec![0; 0x800];
    bytes[..2].copy_from_slice(b"MZ");
    put32(&mut bytes, 0x3c, 0x80);
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    put16(&mut bytes, 0x84, 0x8664);
    put16(&mut bytes, 0x86, 1);
    put16(&mut bytes, 0x94, 240);
    put16(&mut bytes, 0x98, 0x20b);
    put32(&mut bytes, 0xa8, 0x1000);
    put64(&mut bytes, 0xb0, 0x1_4000_0000);
    put32(&mut bytes, 0xb8, 0x1000);
    put32(&mut bytes, 0xbc, 0x200);
    put32(&mut bytes, 0xd0, 0x2000);
    put32(&mut bytes, 0xd4, 0x200);
    put32(&mut bytes, 0x104, 16);
    // Export and ordinary import data directories.
    put32(&mut bytes, 0x108, 0x1200);
    put32(&mut bytes, 0x10c, 0x100);
    put32(&mut bytes, 0x110, 0x1100);
    put32(&mut bytes, 0x114, 40);
    bytes[0x188..0x18d].copy_from_slice(b".text");
    put32(&mut bytes, 0x190, 0x600);
    put32(&mut bytes, 0x194, 0x1000);
    put32(&mut bytes, 0x198, 0x600);
    put32(&mut bytes, 0x19c, 0x200);
    put32(&mut bytes, 0x1ac, 0x6000_0020);
    bytes[0x200..0x208].copy_from_slice(&[0xeb, 4, 0x90, 0x90, 0x90, 0x90, 0xc3, 0x90]);
    put32(&mut bytes, 0x300, 0x1140);
    put32(&mut bytes, 0x30c, 0x1180);
    put32(&mut bytes, 0x310, 0x1160);
    put64(&mut bytes, 0x340, 0x11a0);
    put64(&mut bytes, 0x360, 0x11a0);
    bytes[0x380..0x389].copy_from_slice(b"demo.dll\0");
    bytes[0x3a2..0x3ab].copy_from_slice(b"external\0");
    put32(&mut bytes, 0x40c, 0x1280);
    put32(&mut bytes, 0x410, 1);
    put32(&mut bytes, 0x414, 2);
    put32(&mut bytes, 0x418, 1);
    put32(&mut bytes, 0x41c, 0x1240);
    put32(&mut bytes, 0x420, 0x1250);
    put32(&mut bytes, 0x424, 0x1258);
    put32(&mut bytes, 0x440, 0x1000);
    put32(&mut bytes, 0x444, 0x1290);
    put32(&mut bytes, 0x450, 0x1260);
    bytes[0x460..0x46d].copy_from_slice(b"entry_symbol\0");
    bytes[0x480..0x487].copy_from_slice(b"sample\0");
    bytes[0x490..0x49e].copy_from_slice(b"other.forward\0");
    bytes
}
