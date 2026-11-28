//! Original MIT-licensed fixtures generated from public Apple integer layouts.
//! Constants are literal expected file positions, not parser-derived offsets.

pub const CONTENT: usize = 0x400;
pub const DATA: usize = 0x500;
pub const RELOCATIONS: usize = 0x600;
pub const SYMBOLS: usize = 0x700;
pub const STRINGS: usize = 0x740;
pub const ENTRY: usize = 0x500;

pub fn put32(bytes: &mut [u8], at: usize, value: u32, big: bool) {
    bytes[at..at + 4].copy_from_slice(&if big {
        value.to_be_bytes()
    } else {
        value.to_le_bytes()
    });
}

pub fn put64(bytes: &mut [u8], at: usize, value: u64, big: bool) {
    bytes[at..at + 8].copy_from_slice(&if big {
        value.to_be_bytes()
    } else {
        value.to_le_bytes()
    });
}

pub fn header_size(cpu: u32) -> usize {
    if cpu & 0x0100_0000 != 0 { 32 } else { 28 }
}
pub fn segment_size(cpu: u32) -> usize {
    if cpu & 0x0100_0000 != 0 { 72 } else { 56 }
}
pub fn section_size(cpu: u32) -> usize {
    if cpu & 0x0100_0000 != 0 { 80 } else { 68 }
}
pub fn section_at(cpu: u32, index: usize) -> usize {
    header_size(cpu) + segment_size(cpu) + index * section_size(cpu)
}

fn header(bytes: &mut [u8], cpu: u32, big: bool, file_type: u32, count: u32, commands_size: u32) {
    let wide = cpu & 0x0100_0000 != 0;
    for (at, value) in [
        (0, if wide { 0xfeed_facf } else { 0xfeed_face }),
        (4, cpu),
        (12, file_type),
        (16, count),
        (20, commands_size),
    ] {
        put32(bytes, at, value, big);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn segment_mapping(
    bytes: &mut [u8],
    cpu: u32,
    big: bool,
    address: u64,
    memory: u64,
    offset: u64,
    size: u64,
    flags: u32,
) {
    let at = header_size(cpu);
    if cpu & 0x0100_0000 != 0 {
        for (relative, value) in [(24, address), (32, memory), (40, offset), (48, size)] {
            put64(bytes, at + relative, value, big);
        }
        put32(bytes, at + 68, flags, big);
    } else {
        for (relative, value) in [(24, address), (28, memory), (32, offset), (36, size)] {
            put32(bytes, at + relative, value as u32, big);
        }
        put32(bytes, at + 52, flags, big);
    }
}

fn segment(bytes: &mut [u8], cpu: u32, big: bool, sections: u32) {
    let wide = cpu & 0x0100_0000 != 0;
    let at = header_size(cpu);
    put32(bytes, at, if wide { 0x19 } else { 1 }, big);
    put32(
        bytes,
        at + 4,
        segment_size(cpu) as u32 + sections * section_size(cpu) as u32,
        big,
    );
    put32(bytes, at + if wide { 56 } else { 40 }, 7, big);
    put32(bytes, at + if wide { 60 } else { 44 }, 7, big);
    put32(bytes, at + if wide { 64 } else { 48 }, sections, big);
}

pub fn unix_thread(cpu: u32, big: bool, thumb: bool) -> Vec<u8> {
    let (flavor, count, pc_offset) = match cpu {
        7 => (1, 16, 40),
        0x0100_0007 => (4, 42, 128),
        12 => (1, 17, 60),
        0x0100_000c => (6, 68, 256),
        _ => panic!("fixture requires one of the four baseline CPUs"),
    };
    let mut bytes = vec![0; 0x800];
    let size = 16 + count * 4;
    header(&mut bytes, cpu, big, 2, 2, segment_size(cpu) as u32 + size);
    segment(&mut bytes, cpu, big, 0);
    let at = header_size(cpu);
    bytes[at + 8..at + 14].copy_from_slice(b"__TEXT");
    segment_mapping(&mut bytes, cpu, big, 0x1000, 0x1000, 0, 0x800, 0);
    let thread = at + segment_size(cpu);
    put32(&mut bytes, thread, 5, big);
    put32(&mut bytes, thread + 4, size, big);
    put32(&mut bytes, thread + 8, flavor, big);
    put32(&mut bytes, thread + 12, count, big);
    if cpu & 0x0100_0000 != 0 {
        put64(&mut bytes, thread + 16 + pc_offset, 0x1500, big);
    } else {
        put32(&mut bytes, thread + 16 + pc_offset, 0x1500, big);
    }
    if cpu == 12 {
        put32(
            &mut bytes,
            thread + 16 + 64,
            if thumb { 0x20 } else { 0 },
            big,
        );
        bytes[ENTRY..ENTRY + 4].copy_from_slice(if thumb {
            &[0x00, 0xbf, 0x70, 0x47]
        } else {
            &[0x00, 0x00, 0xa0, 0xe1]
        });
    } else if cpu == 0x0100_000c {
        bytes[ENTRY..ENTRY + 4].copy_from_slice(&[0x1f, 0x20, 0x03, 0xd5]);
    } else {
        bytes[ENTRY..ENTRY + 2].copy_from_slice(&[0x90, 0xc3]);
    }
    bytes
}

pub fn main_entry(cpu: u32, big: bool) -> Vec<u8> {
    let mut bytes = unix_thread(cpu, big, false);
    let at = header_size(cpu) + segment_size(cpu);
    put32(&mut bytes, 20, segment_size(cpu) as u32 + 24, big);
    bytes[at..at + 24].fill(0);
    put32(&mut bytes, at, 0x8000_0028, big);
    put32(&mut bytes, at + 4, 24, big);
    put64(&mut bytes, at + 8, ENTRY as u64, big);
    bytes
}

pub fn object(cpu: u32, big: bool, relocations: &[[u32; 2]]) -> Vec<u8> {
    let wide = cpu & 0x0100_0000 != 0;
    let mut bytes = vec![0; 0x800];
    let command_size = segment_size(cpu) + 2 * section_size(cpu);
    header(&mut bytes, cpu, big, 1, 2, command_size as u32 + 24);
    segment(&mut bytes, cpu, big, 2);
    segment_mapping(
        &mut bytes,
        cpu,
        big,
        0x1000,
        0x300,
        CONTENT as u64,
        0x200,
        0,
    );
    for (index, section_name, segment_name, address, offset, flags) in [
        (
            0,
            b"__text".as_slice(),
            b"__TEXT".as_slice(),
            0x1000_u64,
            CONTENT,
            0x8000_0400,
        ),
        (
            1,
            b"__data".as_slice(),
            b"__DATA".as_slice(),
            0x1100,
            DATA,
            0,
        ),
    ] {
        let at = section_at(cpu, index);
        bytes[at..at + section_name.len()].copy_from_slice(section_name);
        bytes[at + 16..at + 16 + segment_name.len()].copy_from_slice(segment_name);
        if wide {
            put64(&mut bytes, at + 32, address, big);
            put64(&mut bytes, at + 40, 0x80, big);
        } else {
            put32(&mut bytes, at + 32, address as u32, big);
            put32(&mut bytes, at + 36, 0x80, big);
        }
        let tail = if wide { 48 } else { 40 };
        put32(&mut bytes, at + tail, offset as u32, big);
        put32(&mut bytes, at + tail + 4, 2, big);
        if index == 0 {
            put32(&mut bytes, at + tail + 8, RELOCATIONS as u32, big);
            put32(&mut bytes, at + tail + 12, relocations.len() as u32, big);
        }
        put32(&mut bytes, at + tail + 16, flags, big);
    }
    let symtab = header_size(cpu) + command_size;
    for (relative, value) in [
        (0, 2),
        (4, 24),
        (8, SYMBOLS as u32),
        (12, 2),
        (16, STRINGS as u32),
        (20, 32),
    ] {
        put32(&mut bytes, symtab + relative, value, big);
    }
    for (index, string_index, section, value) in [(0, 1, 1, 0x1004), (1, 8, 2, 0x1100)] {
        let at = SYMBOLS + index * if wide { 16 } else { 12 };
        put32(&mut bytes, at, string_index, big);
        bytes[at + 4] = 0x0f;
        bytes[at + 5] = section;
        if wide {
            put64(&mut bytes, at + 8, value, big);
        } else {
            put32(&mut bytes, at + 8, value as u32, big);
        }
    }
    bytes[STRINGS..STRINGS + 16].copy_from_slice(b"\0_entry\0_target\0");
    for (index, words) in relocations.iter().enumerate() {
        put32(&mut bytes, RELOCATIONS + index * 8, words[0], big);
        put32(&mut bytes, RELOCATIONS + index * 8 + 4, words[1], big);
    }
    put32(&mut bytes, CONTENT + 4, 0x1234_5678, big);
    put64(&mut bytes, CONTENT + 16, 0x1122_3344_5566_7788, big);
    bytes
}

pub fn fat_objects(fat64: bool, big: bool) -> Vec<u8> {
    let mut bytes = vec![0; 0x3000];
    put32(
        &mut bytes,
        0,
        if fat64 { 0xcafe_babf } else { 0xcafe_babe },
        big,
    );
    put32(&mut bytes, 4, 2, big);
    for (index, cpu) in [0x0100_0007, 0x0100_000c].into_iter().enumerate() {
        let at = 8 + index * if fat64 { 32 } else { 20 };
        let offset = 0x1000 + index * 0x1000;
        put32(&mut bytes, at, cpu, big);
        if fat64 {
            put64(&mut bytes, at + 8, offset as u64, big);
            put64(&mut bytes, at + 16, 0x800, big);
            put32(&mut bytes, at + 24, 12, big);
        } else {
            put32(&mut bytes, at + 8, offset as u32, big);
            put32(&mut bytes, at + 12, 0x800, big);
            put32(&mut bytes, at + 16, 12, big);
        }
        bytes[offset..offset + 0x800].copy_from_slice(&object(cpu, false, &[[16, 0x0e00_0001]]));
    }
    bytes
}
