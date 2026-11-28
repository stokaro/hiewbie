//! Independently decoded A64 forms absent from the current yaxpeax adapter.
//! The finite scope and independent LLVM inputs are in aarch64-decode-coverage.md.

pub(super) fn decode(word: u32) -> Option<String> {
    let alias = match word {
        0xd503_241f => "bti",
        0xd503_245f => "bti c",
        0xd503_249f => "bti j",
        0xd503_24df => "bti jc",
        0xd503_477f => "smstart",
        0xd503_437f => "smstart sm",
        0xd503_457f => "smstart za",
        0xd503_467f => "smstop",
        0xd503_427f => "smstop sm",
        0xd503_447f => "smstop za",
        _ => "",
    };
    if !alias.is_empty() {
        return Some(alias.into());
    }

    let destination = word & 31;
    let first = (word >> 5) & 31;
    let second = (word >> 16) & 31;
    let element = ELEMENTS[((word >> 22) & 3) as usize];
    if word & 0xff3f_fc10 == 0x2518_e000 {
        let pattern = (word >> 5) & 31;
        let predicate = destination & 15;
        return Some(match pattern {
            31 => format!("ptrue p{predicate}.{element}"),
            29 => format!("ptrue p{predicate}.{element}, mul4"),
            30 => format!("ptrue p{predicate}.{element}, mul3"),
            0 => format!("ptrue p{predicate}.{element}, pow2"),
            1..=13 => {
                const LANES: [u16; 13] = [1, 2, 3, 4, 5, 6, 7, 8, 16, 32, 64, 128, 256];
                format!(
                    "ptrue p{predicate}.{element}, vl{}",
                    LANES[pattern as usize - 1]
                )
            }
            _ => format!("ptrue p{predicate}.{element}, #{pattern}"),
        });
    }
    if let Some(mnemonic) = match word & 0xff20_ec10 {
        0x2520_0400 => Some("whilelt"),
        0x2520_0c00 => Some("whilelo"),
        _ => None,
    } {
        let width = if word & (1 << 12) == 0 { 'w' } else { 'x' };
        let predicate = destination & 15;
        return Some(format!(
            "{mnemonic} p{predicate}.{element}, {}, {}",
            scalar(width, first),
            scalar(width, second)
        ));
    }
    if let Some(mnemonic) = match word & 0xff20_fc00 {
        0x0420_0000 => Some("add"),
        0x0420_0400 => Some("sub"),
        _ => None,
    } {
        return Some(format!(
            "{mnemonic} z{destination}.{element}, z{first}.{element}, z{second}.{element}"
        ));
    }
    if word & 0xffe0_fc00 == 0x0420_3800 {
        return Some(format!(
            "eor3 z{destination}.d, z{destination}.d, z{second}.d, z{first}.d"
        ));
    }
    if let Some(mnemonic) = match word & 0xffa7_fc00 {
        0x4520_4000 => Some("sqxtnb"),
        0x4520_4400 => Some("sqxtnt"),
        _ => None,
    } {
        let size = match word & 0x0058_0000 {
            0x0008_0000 => 0,
            0x0010_0000 => 1,
            0x0040_0000 => 2,
            _ => return None,
        };
        return Some(format!(
            "{mnemonic} z{destination}.{}, z{first}.{}",
            ELEMENTS[size],
            ELEMENTS[size + 1]
        ));
    }
    transfer(word, destination, first)
}

const ELEMENTS: [char; 4] = ['b', 'h', 's', 'd'];

fn scalar(width: char, register: u32) -> String {
    if register == 31 {
        format!("{width}zr")
    } else {
        format!("{width}{register}")
    }
}

fn transfer(word: u32, vector: u32, base: u32) -> Option<String> {
    let (mnemonic, element, zeroing) = match word & 0xfff0_e000 {
        0xa400_a000 => ("ld1b", 'b', "/z"),
        0xa4a0_a000 => ("ld1h", 'h', "/z"),
        0xa540_a000 => ("ld1w", 's', "/z"),
        0xa5e0_a000 => ("ld1d", 'd', "/z"),
        0xe400_e000 => ("st1b", 'b', ""),
        0xe4a0_e000 => ("st1h", 'h', ""),
        0xe540_e000 => ("st1w", 's', ""),
        0xe5e0_e000 => ("st1d", 'd', ""),
        _ => return None,
    };
    let base = if base == 31 {
        "sp".into()
    } else {
        scalar('x', base)
    };
    let predicate = (word >> 10) & 7;
    let displacement = ((word >> 16) & 15) as i32;
    let displacement = (displacement ^ 8) - 8;
    let address = if displacement == 0 {
        base
    } else {
        format!("{base}, #{displacement}, mul vl")
    };
    Some(format!(
        "{mnemonic} {{ z{vector}.{element} }}, p{predicate}{zeroing}, [{address}]"
    ))
}
