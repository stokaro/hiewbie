//! Fixed on-disk thread-state layouts, independent of the inspecting host.

use super::super::{
    EntryMode, Executable, FormatError, MAX_FORMAT_ENTRIES, Machine, Reader, StructureStatus,
    u32_at, u64_at,
};
use crate::ByteSource;

pub(super) struct Entry {
    pub address: u64,
    pub mode: Option<EntryMode>,
    pub field: u64,
}

pub(super) fn aligned(address: u64, mode: Option<EntryMode>) -> bool {
    let mask = match mode {
        Some(EntryMode::Arm) => 3,
        Some(EntryMode::Thumb) => 1,
        None => 0,
    };
    address & mask == 0
}

pub(super) struct Command {
    pub entry: Option<Entry>,
    pub status: StructureStatus,
    pub detail: String,
}

// Apple's thread_status.h counts 32-bit words, including in 64-bit states.
// PC offsets come from the matching _structs.h, never Rust/C host layouts.
pub(super) fn parse<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &mut Executable,
    at: u64,
    size: u64,
    index: usize,
) -> Result<Command, FormatError> {
    if size < 16 {
        return Err(reader.invalid("Mach-O thread command has no complete state header", at));
    }
    let mut result = Command {
        entry: None,
        status: StructureStatus::Interpreted,
        detail: String::new(),
    };
    let mut cursor = at + 8;
    let end = at + size;
    let mut states = 0;
    while cursor < end {
        if end - cursor == 4 && image.bitness == 64 && reader.read::<4>(cursor)? == [0; 4] {
            break;
        }
        if end - cursor < 8 {
            return Err(reader.invalid("Mach-O thread-state header is truncated", cursor));
        }
        if states >= MAX_FORMAT_ENTRIES {
            return Err(FormatError::Limit {
                resource: "Mach-O thread states",
                limit: MAX_FORMAT_ENTRIES,
            });
        }
        let header = reader.read::<8>(cursor)?;
        let flavor = u32_at(&header, 0, image.endian);
        let count = u32_at(&header, 4, image.endian);
        let bytes = u64::from(count) * 4;
        if bytes > end - cursor - 8 {
            return Err(reader.invalid("Mach-O thread-state count exceeds its command", cursor + 4));
        }
        let label = format!("Command[{index}].Thread[{states}]");
        reader.field(
            image,
            format!("{label}.Flavor"),
            u64::from(flavor),
            cursor,
            4,
        )?;
        reader.field(
            image,
            format!("{label}.Count"),
            u64::from(count),
            cursor + 4,
            4,
        )?;
        let layout = match (image.machine, flavor) {
            (Machine::X86, 1) => Some((16, 40, 4, "x86_THREAD_STATE32")),
            (Machine::X86_64, 4) => Some((42, 128, 8, "x86_THREAD_STATE64")),
            (Machine::Arm, 1 | 9) => Some((17, 60, 4, "ARM_THREAD_STATE32")),
            (Machine::Aarch64, 6) => Some((68, 256, 8, "ARM_THREAD_STATE64")),
            _ => None,
        };
        if let Some((expected, pc_offset, width, layout_name)) = layout {
            if count != expected {
                return Err(
                    reader.invalid("Mach-O known thread-state count is invalid", cursor + 4)
                );
            }
            if result.entry.is_some() {
                return Err(reader.invalid(
                    "Mach-O thread command has multiple program-counter states",
                    cursor,
                ));
            }
            let field = cursor + 8 + pc_offset;
            let address = if width == 8 {
                u64_at(&reader.read::<8>(field)?, 0, image.endian)
            } else {
                u64::from(u32_at(&reader.read::<4>(field)?, 0, image.endian))
            };
            reader.field(image, format!("{label}.PC"), address, field, width)?;
            let mode = if image.machine == Machine::Arm {
                let cpsr_at = cursor + 8 + 64;
                let cpsr = u32_at(&reader.read::<4>(cpsr_at)?, 0, image.endian);
                reader.field(image, format!("{label}.CPSR"), u64::from(cpsr), cpsr_at, 4)?;
                Some(if cpsr & 0x20 == 0 {
                    EntryMode::Arm
                } else {
                    EntryMode::Thumb
                })
            } else {
                None
            };
            result.entry = Some(Entry {
                address,
                mode,
                field,
            });
            result.detail = format!("{layout_name}, PC=0x{address:X}");
            if let Some(mode) = mode {
                result
                    .detail
                    .push_str(&format!(", CPSR execution state={mode:?}"));
            }
            if !aligned(address, mode) {
                result
                    .detail
                    .push_str("; misaligned PC for the declared execution state; no target");
            }
        } else {
            result.status = StructureStatus::Unsupported;
            reader.note(image, format!(
                "Command[{index}] thread flavor {flavor} for {} is unsupported; its {count} words remain in the raw command.",
                image.machine,
            ))?;
        }
        cursor += 8 + bytes;
        states += 1;
    }
    if result.detail.is_empty() {
        result.detail =
            "No supported program-counter state; raw flavor/count records remain available".into();
    }
    if result.status == StructureStatus::Unsupported {
        result
            .detail
            .push_str("; additional unsupported thread-state data remains raw");
    }
    Ok(result)
}
