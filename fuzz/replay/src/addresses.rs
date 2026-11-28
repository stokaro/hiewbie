use hiewbie_core::formats::{
    AddressError, BinaryFormat, Endian, Executable, FileRange, Machine, Region, Rva, SliceOffset,
    VirtualAddress,
};
use hiewbie_core::{FileOffset, parse_goto, parse_virtual_address};

use crate::word;

pub(crate) fn check(data: &[u8]) {
    let regions: Vec<_> = data
        .get(49..)
        .unwrap_or_default()
        .chunks(32)
        .take(8)
        .map(|bytes| Region {
            name: String::new(),
            file: FileRange {
                offset: FileOffset::new(word(bytes, 0)),
                len: word(bytes, 8),
            },
            address: VirtualAddress::new(word(bytes, 16)),
            memory_size: word(bytes, 24),
            readable: true,
            writable: false,
            executable: false,
            header: FileRange {
                offset: FileOffset::new(0),
                len: 0,
            },
        })
        .collect();
    let image = Executable {
        generation: 0,
        format: BinaryFormat::Elf64,
        machine: Machine::X86_64,
        endian: Endian::Little,
        bitness: 64,
        image_base: (data.get(48).copied().unwrap_or(0) & 1 == 0)
            .then_some(VirtualAddress::new(word(data, 0))),
        entry: None,
        slice: FileRange {
            offset: FileOffset::new(word(data, 8)),
            len: word(data, 16),
        },
        selected_slice: None,
        slices: Vec::new(),
        regions,
        fields: Vec::new(),
    };
    let offset = word(data, 24);
    let address = word(data, 32);
    // The oracle uses wide integer interval arithmetic, independently of the
    // core's checked-sub/add implementation and newtype conversions.
    let file_matches: Vec<u128> = image
        .regions
        .iter()
        .filter_map(|region| {
            let start = u128::from(region.file.offset.get());
            let query = u128::from(offset);
            (query >= start && query < start + u128::from(region.file.len.min(region.memory_size)))
                .then(|| u128::from(region.address.get()) + query - start)
        })
        .collect();
    let actual = image.file_to_va(FileOffset::new(offset));
    if file_matches
        .iter()
        .any(|value| *value > u128::from(u64::MAX))
    {
        assert!(actual.is_err());
    } else {
        let expected = match file_matches.as_slice() {
            [] => Err(AddressError::Unmapped),
            [value] => Ok(VirtualAddress::new(*value as u64)),
            _ => Err(AddressError::Ambiguous),
        };
        assert_eq!(actual, expected);
    }

    let virtual_matches: Vec<_> = image
        .regions
        .iter()
        .filter_map(|region| {
            let query = u128::from(address);
            let start = u128::from(region.address.get());
            if query < start || query >= start + u128::from(region.memory_size) {
                return None;
            }
            let delta = query - start;
            Some(if delta >= u128::from(region.file.len) {
                Err(AddressError::ZeroFill)
            } else {
                let at = u128::from(region.file.offset.get()) + delta;
                u64::try_from(at)
                    .map(FileOffset::new)
                    .map_err(|_| AddressError::Overflow)
            })
        })
        .collect();
    let expected = match virtual_matches.as_slice() {
        [] => Err(AddressError::Unmapped),
        [value] => *value,
        _ => Err(AddressError::Ambiguous),
    };
    assert_eq!(image.va_to_file(VirtualAddress::new(address)), expected);

    let relative = word(data, 40);
    let expected = image
        .image_base
        .ok_or(AddressError::NoImageBase)
        .and_then(|base| {
            u64::try_from(u128::from(base.get()) + u128::from(relative))
                .map(VirtualAddress::new)
                .map_err(|_| AddressError::Overflow)
        });
    assert_eq!(image.rva_to_va(Rva::new(relative)), expected);
    if let Ok(value) = expected {
        assert_eq!(image.va_to_rva(value), Ok(Rva::new(relative)));
    }
    let slice_position = u128::from(image.slice.offset.get()) + u128::from(relative);
    let expected = if relative >= image.slice.len {
        Err(AddressError::Unmapped)
    } else {
        u64::try_from(slice_position)
            .map(FileOffset::new)
            .map_err(|_| AddressError::Overflow)
    };
    assert_eq!(image.slice_to_file(SliceOffset::new(relative)), expected);
    if let Ok(value) = expected {
        assert_eq!(image.file_to_slice(value), Ok(SliceOffset::new(relative)));
    }

    let len = word(data, 16);
    let current = FileOffset::new(offset);
    for input in [
        format!("{relative:x}"),
        format!("{relative}t"),
        format!("+{relative:x}"),
        format!("-{relative:x}"),
    ] {
        let result = parse_goto(&input, current, len);
        let candidate = if input.starts_with('+') {
            Some(u128::from(offset) + u128::from(relative))
        } else if input.starts_with('-') {
            u128::from(offset).checked_sub(u128::from(relative))
        } else {
            Some(u128::from(relative))
        };
        let expected = candidate.filter(|value| {
            *value <= u128::from(u64::MAX)
                && if len == 0 {
                    *value == 0
                } else {
                    *value < u128::from(len)
                }
        });
        assert_eq!(result.ok().map(|value| u128::from(value.get())), expected);
    }
    let input = String::from_utf8_lossy(data);
    if let Ok(position) = parse_goto(&input, current, len) {
        assert!(if len == 0 {
            position.get() == 0
        } else {
            position.get() < len
        });
    }
    let _ = parse_virtual_address(&input);
    assert_eq!(
        parse_virtual_address(&format!(".{relative:X}"))
            .unwrap()
            .get(),
        relative
    );
}
