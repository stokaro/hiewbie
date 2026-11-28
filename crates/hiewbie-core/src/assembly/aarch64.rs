use super::AssemblyError;
use super::syntax::expression;
use crate::formats::VirtualAddress;

mod memory;
mod relative;
mod scalar;
mod vector;

pub(super) fn encode(
    source: &str,
    address: VirtualAddress,
    mut resolve: impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    if !address.get().is_multiple_of(4) {
        return Err(AssemblyError::OutOfRange(
            "AArch64 instruction addresses must be four-byte aligned",
        ));
    }
    if !source
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b" \t.,#[]!+_@$?`-{} /;".contains(&byte))
    {
        return Err(AssemblyError::Syntax(
            "Expected one ASCII AArch64 instruction",
        ));
    }
    let source = source
        .split_once(';')
        .map_or(source, |(code, _)| code)
        .trim();
    let (mnemonic, operands) = source
        .split_once(char::is_whitespace)
        .unwrap_or((source, ""));
    if mnemonic.is_empty() || !mnemonic.as_bytes()[0].is_ascii_alphabetic() {
        return Err(AssemblyError::Syntax("Expected an instruction mnemonic"));
    }
    let mnemonic = mnemonic.to_ascii_lowercase();
    let operands = split_operands(operands)?;
    let args = operands.as_slice();
    match mnemonic.as_str() {
        "b" | "bl" | "cbz" | "cbnz" | "tbz" | "tbnz" | "adr" | "adrp" => {
            relative::encode(&mnemonic, args, address.get(), &mut resolve)
        }
        m if m.starts_with("b.") || m.starts_with("bc.") => {
            relative::encode(m, args, address.get(), &mut resolve)
        }
        "ldr" | "ldrsw" | "prfm" if args.get(1).is_some_and(|s| !s.starts_with('[')) => {
            relative::encode(&mnemonic, args, address.get(), &mut resolve)
        }
        "ldr" | "ldrb" | "ldrh" | "ldrsw" | "ldur" | "ldurb" | "ldurh" | "ldursw" | "str"
        | "strb" | "strh" | "stur" | "sturb" | "sturh" | "ldp" | "stp" => {
            memory::encode(&mnemonic, args, &mut resolve)
        }
        "ldadd" | "ldadda" | "ldaddl" | "ldaddal" => memory::atomic(&mnemonic, args),
        "ldraa" | "ldrab" | "stg" => memory::authenticated_or_tag(&mnemonic, args, &mut resolve),
        "dmb" | "dsb" | "isb" | "mrs" | "msr" | "irg" => scalar::system(&mnemonic, args),
        "ptrue" | "whilelt" | "whilelo" | "eor3" | "sqxtnb" | "sqxtnt" | "ld1b" | "ld1h"
        | "ld1w" | "ld1d" | "st1b" | "st1h" | "st1w" | "st1d" | "aese" | "aesd" | "aesmc"
        | "aesimc" | "sha256h" | "sha256h2" | "fadd" | "fsub" | "fmul" | "fdiv" => {
            vector::encode(&mnemonic, args, &mut resolve)
        }
        "add" | "sub"
            if args.first().is_some_and(|s| {
                s.contains('.') && matches!(s.as_bytes().first(), Some(b'v' | b'V' | b'z' | b'Z'))
            }) =>
        {
            vector::encode(&mnemonic, args, &mut resolve)
        }
        "br" | "blr" | "ret" | "brk" | "svc" | "nop" | "mov" | "movk" | "movn" | "movz" | "add"
        | "adds" | "sub" | "subs" | "cmp" | "cmn" | "and" | "ands" | "orr" | "eor" | "tst"
        | "lsl" | "lsr" | "asr" | "ror" | "mul" | "madd" | "msub" | "sdiv" | "udiv" | "paciasp"
        | "pacibsp" | "autiasp" | "autibsp" | "retaa" | "retab" | "braa" | "brab" | "blraa"
        | "blrab" | "bti" | "smstart" | "smstop" | "crc32x" | "crc32cx" => {
            scalar::encode(&mnemonic, args, &mut resolve)
        }
        _ => Err(AssemblyError::UnknownMnemonic(mnemonic)),
    }
}

/// Split top-level operands and retain memory and vector-list punctuation
/// for the corresponding typed parser.
fn split_operands(source: &str) -> Result<Vec<&str>, AssemblyError> {
    let mut operands = Vec::new();
    let mut start = 0;
    let mut closing = None;
    let source = source.trim();
    if source.is_empty() {
        return Ok(operands);
    }
    for (index, byte) in source.bytes().enumerate() {
        match (closing, byte) {
            (None, b'[') => closing = Some(b']'),
            (None, b'{') => closing = Some(b'}'),
            (Some(expected), byte) if expected == byte => closing = None,
            (_, b'[' | b']' | b'{' | b'}') => {
                return Err(AssemblyError::Syntax("Unbalanced AArch64 operand brackets"));
            }
            (None, b',') => {
                operands.push(source[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    operands.push(source[start..].trim());
    if closing.is_some() || operands.len() > 5 || operands.iter().any(|s| s.is_empty()) {
        return Err(AssemblyError::Syntax("Invalid AArch64 operand list"));
    }
    Ok(operands)
}

#[derive(Clone, Copy)]
struct Gpr {
    index: u32,
    wide: bool,
    stack: bool,
}

impl Gpr {
    const fn width(self) -> u32 {
        if self.wide { 64 } else { 32 }
    }

    const fn sf(self) -> u32 {
        (self.wide as u32) << 31
    }

    fn plain(self) -> Result<Self, AssemblyError> {
        if self.stack {
            return Err(AssemblyError::UnsupportedForm(
                "This operand requires a data register, not SP",
            ));
        }
        Ok(self)
    }

    fn address(self) -> Result<Self, AssemblyError> {
        if self.wide && (self.index != 31 || self.stack) {
            Ok(self)
        } else {
            Err(AssemblyError::UnsupportedForm(
                "Expected a 64-bit address register or SP",
            ))
        }
    }

    fn same_width(self, other: Self) -> Result<(), AssemblyError> {
        if self.wide == other.wide {
            Ok(())
        } else {
            Err(AssemblyError::UnsupportedForm(
                "AArch64 register widths must match",
            ))
        }
    }
}

fn gpr(text: &str) -> Result<Gpr, AssemblyError> {
    let lower = text.to_ascii_lowercase();
    let (wide, index, stack) = match lower.as_str() {
        "sp" => (true, 31, true),
        "wsp" => (false, 31, true),
        "xzr" => (true, 31, false),
        "wzr" => (false, 31, false),
        "fp" => (true, 29, false),
        "lr" => (true, 30, false),
        _ => {
            let (wide, rest) = if let Some(rest) = lower.strip_prefix('x') {
                (true, rest)
            } else if let Some(rest) = lower.strip_prefix('w') {
                (false, rest)
            } else {
                return Err(AssemblyError::Syntax("Expected an X or W register"));
            };
            (wide, reg_index(rest, 30)?, false)
        }
    };
    Ok(Gpr { index, wide, stack })
}

fn reg_index(text: &str, maximum: u32) -> Result<u32, AssemblyError> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AssemblyError::Syntax("Invalid register number"));
    }
    text.parse::<u32>()
        .ok()
        .filter(|index| *index <= maximum)
        .ok_or(AssemblyError::OutOfRange(
            "AArch64 register number is out of range",
        ))
}

fn operand_count<T>() -> Result<T, AssemblyError> {
    Err(AssemblyError::Syntax("Wrong number of AArch64 operands"))
}

fn number(
    source: &str,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<i128, AssemblyError> {
    expression(source.strip_prefix('#').unwrap_or(source), resolve)
}

fn unsigned(
    source: &str,
    bits: u32,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u64, AssemblyError> {
    let value = number(source, resolve)?;
    if !(0..1_i128 << bits).contains(&value) {
        return Err(AssemblyError::OutOfRange(
            "AArch64 immediate is out of range",
        ));
    }
    Ok(value as u64)
}

fn signed_field(value: i128, bits: u32) -> Result<u32, AssemblyError> {
    let half = 1_i128 << (bits - 1);
    if !(-half..half).contains(&value) {
        return Err(AssemblyError::OutOfRange(
            "AArch64 signed displacement is out of range",
        ));
    }
    Ok(value as u32 & ((1 << bits) - 1))
}

fn sized_field(value: i128, bits: u32, scale: i128) -> Result<u32, AssemblyError> {
    if value % scale != 0 {
        return Err(AssemblyError::OutOfRange(
            "AArch64 displacement is unaligned",
        ));
    }
    signed_field(value / scale, bits)
}
