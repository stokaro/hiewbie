use iced_x86::{Mnemonic, Register};

use super::AssemblyError;
use crate::formats::VirtualAddress;

#[derive(Clone, Copy)]
pub(super) enum Operand {
    Register(Register),
    Number(i128),
    Memory(Memory),
    Far { selector: i128, offset: i128 },
}

#[derive(Clone, Copy)]
pub(super) struct Memory {
    pub base: Register,
    pub index: Register,
    pub scale: u32,
    pub displacement: i128,
    pub size: Option<usize>,
    pub segment: Register,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BranchForm {
    Any,
    Short,
    FarMemory,
}

#[derive(Clone, Copy)]
pub(super) enum Prefix {
    None,
    Lock,
    Rep,
    Repne,
}

pub(super) struct Source {
    pub mnemonic: Mnemonic,
    pub branch_form: BranchForm,
    pub prefix: Prefix,
    pub operands: Vec<Operand>,
}

pub(super) fn parse(
    text: &str,
    mut resolve: impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<Source, AssemblyError> {
    if !text
        .bytes()
        .all(|byte| byte == b'\t' || (b' '..=b'~').contains(&byte))
    {
        return Err(AssemblyError::Syntax("Enter one ASCII instruction"));
    }
    let text = &text[..outside_quotes(text, b";").next().unwrap_or(text.len())];
    let (word, rest) = word(text.trim());
    let (prefix, name, rest) = match word.to_ascii_lowercase().as_str() {
        "lock" => prefixed(Prefix::Lock, rest),
        "rep" | "repe" | "repz" => prefixed(Prefix::Rep, rest),
        "repne" | "repnz" => prefixed(Prefix::Repne, rest),
        _ => (Prefix::None, word, rest),
    };
    let (canonical, branch_form) = match name.to_ascii_lowercase().as_str() {
        "jmps" => ("jmp", BranchForm::Short),
        "jmpf" => ("jmp", BranchForm::FarMemory),
        "callf" => ("call", BranchForm::FarMemory),
        "jz" => ("je", BranchForm::Any),
        "jnz" => ("jne", BranchForm::Any),
        "jc" => ("jb", BranchForm::Any),
        "jnc" => ("jae", BranchForm::Any),
        "sal" => ("shl", BranchForm::Any),
        "retn" => ("ret", BranchForm::Any),
        _ => (name, BranchForm::Any),
    };
    if name.is_empty() {
        return Err(AssemblyError::Syntax("Enter an instruction mnemonic"));
    }
    let mnemonic = Mnemonic::values()
        .skip(1)
        .find(|mnemonic| format!("{mnemonic:?}").eq_ignore_ascii_case(canonical))
        .ok_or_else(|| AssemblyError::UnknownMnemonic(name.into()))?;
    let mut operands = Vec::new();
    if !rest.is_empty() {
        let mut start = 0;
        let mut parts = outside_quotes(rest, b",")
            .chain(std::iter::once(rest.len()))
            .map(|end| {
                let part = rest[start..end].trim();
                start = end + 1;
                part
            })
            .peekable();
        while let Some(part) = parts.next() {
            if operands.len() == 5 {
                return Err(AssemblyError::Syntax("Too many operands"));
            }
            let size = hiew_size(part);
            if let Some(size) = size
                && parts
                    .peek()
                    .is_some_and(|next| outside_quotes(next, b"[").next().is_some())
            {
                let next = parts
                    .next()
                    .ok_or(AssemblyError::Syntax("Expected a sized memory operand"))?;
                let Operand::Memory(mut memory) = parse_operand(next, &mut resolve)? else {
                    return Err(AssemblyError::Syntax("Expected a sized memory operand"));
                };
                if memory.size.replace(size).is_some() {
                    return Err(AssemblyError::Syntax(
                        "Specify one size for the memory operand",
                    ));
                }
                operands.push(Operand::Memory(memory));
            } else {
                operands.push(parse_operand(part, &mut resolve)?);
            }
        }
    }
    if branch_form == BranchForm::FarMemory
        && matches!(&operands[..], [Operand::Memory(Memory { size: None, .. })])
    {
        return Err(AssemblyError::AmbiguousMemorySize);
    }
    Ok(Source {
        mnemonic,
        branch_form,
        prefix,
        operands,
    })
}

fn word(text: &str) -> (&str, &str) {
    let split = text.find([' ', '\t']).unwrap_or(text.len());
    (&text[..split], text[split..].trim())
}

fn prefixed(prefix: Prefix, rest: &str) -> (Prefix, &str, &str) {
    let (name, operands) = word(rest);
    (prefix, name, operands)
}

fn outside_quotes<'a>(text: &'a str, wanted: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    text.bytes()
        .enumerate()
        .scan(None, move |quote, (index, byte)| {
            let delimiter = match *quote {
                Some(end) => {
                    if byte == end {
                        *quote = None;
                    }
                    false
                }
                None if matches!(byte, b'\'' | b'"') => {
                    *quote = Some(byte);
                    false
                }
                None => wanted.contains(&byte),
            };
            Some(delimiter.then_some(index))
        })
        .flatten()
}

fn parse_operand(
    text: &str,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<Operand, AssemblyError> {
    if let Some(bracket) = outside_quotes(text, b"[").next() {
        return parse_memory(&text[..bracket], &text[bracket + 1..], resolve).map(Operand::Memory);
    }
    if let Some(colon) = outside_quotes(text, b":").next() {
        return Ok(Operand::Far {
            selector: expression(&text[..colon], resolve)?,
            offset: expression(&text[colon + 1..], resolve)?,
        });
    }
    if let Some(register) = register(text) {
        return Ok(Operand::Register(register));
    }
    expression(text, resolve).map(Operand::Number)
}

fn hiew_size(text: &str) -> Option<usize> {
    match text.as_bytes() {
        [b'b' | b'B'] => Some(1),
        [b'w' | b'W'] => Some(2),
        [b'd' | b'D'] => Some(4),
        [b'f' | b'F'] => Some(6),
        [b'q' | b'Q'] => Some(8),
        [b't' | b'T'] => Some(10),
        _ => None,
    }
}

fn parse_memory(
    qualifier: &str,
    body: &str,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<Memory, AssemblyError> {
    let mut qualifier = qualifier.trim().to_ascii_lowercase();
    let mut segment = Register::None;
    if let Some(before_colon) = qualifier.strip_suffix(':') {
        let split = before_colon.rfind([' ', '\t']).map_or(0, |i| i + 1);
        segment = register(&before_colon[split..])
            .filter(|register| register.is_segment_register())
            .ok_or(AssemblyError::Syntax("Invalid segment register"))?;
        qualifier.truncate(split);
    }
    let qualifier = qualifier.trim();
    let size = match qualifier.strip_suffix(" ptr").unwrap_or(qualifier) {
        "" => None,
        "b" | "byte" => Some(1),
        "w" | "word" => Some(2),
        "d" | "dword" => Some(4),
        "fword" => Some(6),
        "q" | "qword" => Some(8),
        "tbyte" => Some(10),
        "oword" | "xmmword" => Some(16),
        "yword" | "ymmword" => Some(32),
        _ => return Err(AssemblyError::Syntax("Invalid memory operand size")),
    };
    let body = body
        .strip_suffix(']')
        .ok_or(AssemblyError::Syntax("Missing closing bracket"))?;
    let mut memory = Memory {
        base: Register::None,
        index: Register::None,
        scale: 1,
        displacement: 0,
        size,
        segment,
    };
    for (subtract, term) in terms(body)? {
        let scaled = term.split_once('*');
        if let Some((index, scale)) = scaled {
            if subtract || memory.index != Register::None {
                return Err(AssemblyError::Syntax("Unexpected scaled index"));
            }
            memory.index = register(index.trim())
                .filter(|register| register.is_gpr())
                .ok_or(AssemblyError::Syntax("Invalid index register"))?;
            memory.scale = match scale.trim() {
                "1" => 1,
                "2" => 2,
                "4" => 4,
                "8" => 8,
                _ => return Err(AssemblyError::Syntax("Index scale must be 1, 2, 4, or 8")),
            };
        } else if let Some(reg) = register(term) {
            if subtract || !(reg.is_gpr() || reg == Register::RIP) {
                return Err(AssemblyError::Syntax("Invalid address register"));
            }
            if memory.base == Register::None {
                memory.base = reg;
            } else if memory.index == Register::None {
                memory.index = reg;
            } else {
                return Err(AssemblyError::Syntax("Too many address registers"));
            }
        } else {
            memory.displacement = add_term(memory.displacement, atom(term, resolve)?, subtract)?;
        }
    }
    Ok(memory)
}

pub(super) fn expression(
    text: &str,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<i128, AssemblyError> {
    terms(text)?
        .into_iter()
        .try_fold(0, |value, (subtract, term)| {
            add_term(value, atom(term, resolve)?, subtract)
        })
}

fn terms(text: &str) -> Result<Vec<(bool, &str)>, AssemblyError> {
    let text = text.trim();
    let subtract = text.starts_with('-');
    let text = text.strip_prefix(['+', '-']).unwrap_or(text);
    let mut current_sign = subtract;
    let mut start = 0;
    let mut terms = Vec::new();
    for end in outside_quotes(text, b"+-").chain(std::iter::once(text.len())) {
        let term = text[start..end].trim();
        if term.is_empty() {
            return Err(AssemblyError::Syntax(
                "Expected a value after the operand separator",
            ));
        }
        terms.push((current_sign, term));
        current_sign = text.as_bytes().get(end) == Some(&b'-');
        start = end + 1;
    }
    Ok(terms)
}

fn add_term(value: i128, term: i128, subtract: bool) -> Result<i128, AssemblyError> {
    let result = if subtract {
        value.checked_sub(term)
    } else {
        value.checked_add(term)
    };
    result.ok_or(AssemblyError::OutOfRange("Constant expression overflow"))
}

fn atom(
    term: &str,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<i128, AssemblyError> {
    if let Some(quote) = term.chars().next().filter(|ch| matches!(ch, '\'' | '"')) {
        let body = term[1..]
            .strip_suffix(quote)
            .filter(|body| !body.is_empty() && !body.contains(quote))
            .ok_or(AssemblyError::Syntax("Invalid quoted immediate"))?;
        let mut bytes = [0; 8];
        let dest = bytes
            .get_mut(..body.len())
            .ok_or(AssemblyError::OutOfRange(
                "Quoted immediate exceeds eight bytes",
            ))?;
        dest.copy_from_slice(body.as_bytes());
        return Ok(i128::from(u64::from_le_bytes(bytes)));
    }
    if let Some((high, low)) = term.split_once('`') {
        if (1..=8).contains(&high.len())
            && low.len() == 8
            && high
                .bytes()
                .chain(low.bytes())
                .all(|byte| byte.is_ascii_hexdigit())
        {
            let high = u64::from_str_radix(high, 16)
                .map_err(|_| AssemblyError::Syntax("Invalid high word"))?;
            let low = u64::from_str_radix(low, 16)
                .map_err(|_| AssemblyError::Syntax("Invalid low word"))?;
            return Ok(i128::from(high << 32 | low));
        }
        return Err(AssemblyError::Syntax(
            "Expected hexadecimal high and low address words",
        ));
    }
    let lower = term.to_ascii_lowercase();
    let (digits, radix) = if let Some(digits) = lower.strip_prefix("0x") {
        (digits, 16)
    } else if let Some(digits) = lower.strip_suffix('t') {
        (digits, 10)
    } else if let Some(digits) = lower.strip_suffix('h') {
        (digits, 16)
    } else {
        (lower.as_str(), 16)
    };
    if !digits.is_empty() && digits.chars().all(|ch| ch.is_digit(radix)) {
        return u64::from_str_radix(digits, radix)
            .map(i128::from)
            .map_err(|_| AssemblyError::OutOfRange("Numeric operand exceeds 64 bits"));
    }
    let name = term.strip_prefix('@').unwrap_or(term);
    let allowed = |ch: u8| ch.is_ascii_alphabetic() || matches!(ch, b'_' | b'.' | b'$' | b'?');
    if name.as_bytes().first().copied().is_some_and(allowed)
        && name.bytes().all(|ch| allowed(ch) || ch.is_ascii_digit())
    {
        return resolve(name)
            .map(|address| i128::from(address.get()))
            .ok_or_else(|| AssemblyError::UnresolvedSymbol(name.into()));
    }
    Err(AssemblyError::Syntax("Invalid numeric operand or symbol"))
}

fn register(text: &str) -> Option<Register> {
    let lower = text.to_ascii_lowercase();
    let st = lower
        .strip_prefix("st(")
        .and_then(|number| number.strip_suffix(')'));
    Register::values().skip(1).find(|reg| {
        if let Some(number) = st {
            reg.is_st() && number.parse::<usize>() == Ok(reg.number())
        } else {
            format!("{reg:?}").eq_ignore_ascii_case(text)
        }
    })
}
