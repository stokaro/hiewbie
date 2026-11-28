//! Bounded 64-bit expressions evaluated against logical document bytes.

use std::fmt;

use crate::formats::Endian;
use crate::{ByteSource, Document, Error, FileOffset};

pub const MAX_EXPRESSION_BYTES: usize = 512;
const MAX_EXPRESSION_DEPTH: usize = 64;

#[derive(Debug)]
pub enum CalculationError {
    InvalidOperator {
        at: usize,
    },
    InvalidTag {
        at: usize,
    },
    InvalidByteOrder {
        at: usize,
    },
    MissingBracket {
        at: usize,
    },
    BigNumber {
        at: usize,
    },
    LengthLimit,
    NestingLimit,
    Syntax {
        at: usize,
        expected: &'static str,
    },
    InvalidNumber {
        at: usize,
    },
    DivisionByZero {
        at: usize,
    },
    ArithmeticOverflow {
        at: usize,
    },
    InvalidShiftCount {
        at: usize,
    },
    LocalOffsetOverflow {
        at: usize,
    },
    ShortRead {
        at: usize,
        offset: FileOffset,
        width: usize,
    },
    Read(Error),
}

impl fmt::Display for CalculationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTag { at } => write!(f, "Invalid cursor tag at column {}", at + 1),
            Self::InvalidByteOrder { at } => write!(
                f,
                "Invalid byte order: use :le/:be on value tags (column {})",
                at + 1
            ),
            Self::InvalidOperator { at } => write!(f, "Invalid operator at column {}", at + 1),
            Self::MissingBracket { at } => write!(f, "No closing parenthesis at column {}", at + 1),
            Self::BigNumber { at } => write!(f, "Integer exceeds 64 bits at column {}", at + 1),
            Self::LengthLimit => write!(f, "Expression exceeds {MAX_EXPRESSION_BYTES} bytes"),
            Self::NestingLimit => write!(f, "Expression nesting exceeds {MAX_EXPRESSION_DEPTH}"),
            Self::Syntax { at, expected } => write!(f, "Column {} expects {expected}", at + 1),
            Self::InvalidNumber { at } => write!(f, "Invalid 64-bit integer at column {}", at + 1),
            Self::DivisionByZero { .. } => f.write_str("Division by zero"),
            Self::ArithmeticOverflow { .. } => f.write_str("Signed 64-bit arithmetic overflow"),
            Self::InvalidShiftCount { .. } => f.write_str("Shift count must be between 0 and 63"),
            Self::LocalOffsetOverflow { .. } => f.write_str("Local address exceeds 64 bits"),
            Self::ShortRead { offset, width, .. } => {
                write!(f, "Need {width} bytes at file offset {:X}", offset.get())
            }
            Self::Read(error) => write!(f, "Calculator read failed: {error}"),
        }
    }
}

impl CalculationError {
    /// Position in the expression for the diagnostic marker, when available.
    pub fn expression_offset(&self) -> Option<usize> {
        match self {
            Self::InvalidOperator { at }
            | Self::InvalidTag { at }
            | Self::InvalidByteOrder { at }
            | Self::MissingBracket { at }
            | Self::BigNumber { at }
            | Self::Syntax { at, .. }
            | Self::InvalidNumber { at }
            | Self::DivisionByZero { at }
            | Self::ArithmeticOverflow { at }
            | Self::InvalidShiftCount { at }
            | Self::LocalOffsetOverflow { at }
            | Self::ShortRead { at, .. } => Some(*at),
            _ => None,
        }
    }
}

impl std::error::Error for CalculationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            _ => None,
        }
    }
}

/// Evaluate one expression without moving or modifying the document.
/// Typed reads use little-endian values at `cursor` unless the tag has an
/// explicit `:le` or `:be` suffix (ASCII case-insensitive). Lowercase tags
/// sign-extend and uppercase tags zero-extend 8/16/32/64-bit values.
/// The byte-order suffix is a hiewbie extension to the original cursor tags.
/// The caller supplies the local coordinate for `@o`, or `None` when the local
/// address overflows. `@O` always yields `cursor`.
pub fn evaluate<S: ByteSource>(
    expression: &str,
    document: &mut Document<S>,
    cursor: FileOffset,
    local: Option<u64>,
) -> Result<u64, CalculationError> {
    evaluate_with(expression, document, cursor, local, Arithmetic::Wrapping)
}

/// Evaluate the same expression with checked signed 64-bit arithmetic.
/// Bitwise operations use the original bit patterns and shifts require a count below 64.
pub fn evaluate_checked<S: ByteSource>(
    expression: &str,
    document: &mut Document<S>,
    cursor: FileOffset,
    local: Option<u64>,
) -> Result<u64, CalculationError> {
    evaluate_with(expression, document, cursor, local, Arithmetic::Checked)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arithmetic {
    Wrapping,
    Checked,
}

fn evaluate_with<S: ByteSource>(
    expression: &str,
    document: &mut Document<S>,
    cursor: FileOffset,
    local: Option<u64>,
    arithmetic: Arithmetic,
) -> Result<u64, CalculationError> {
    if expression.len() > MAX_EXPRESSION_BYTES {
        return Err(CalculationError::LengthLimit);
    }

    let mut parser = Parser {
        input: expression,
        index: 0,
        document,
        cursor,
        local,
        arithmetic,
    };
    let value = parser.expression(1, 0)?;
    match parser.rest().as_bytes().first() {
        Some(byte) if byte.is_ascii_hexdigit() => return Err(parser.expected("an operator")),
        Some(_) => return Err(CalculationError::InvalidOperator { at: parser.index }),
        None => {}
    }
    Ok(value)
}

struct Parser<'a, S: ByteSource> {
    input: &'a str,
    index: usize,
    document: &'a mut Document<S>,
    cursor: FileOffset,
    local: Option<u64>,
    arithmetic: Arithmetic,
}

impl<S: ByteSource> Parser<'_, S> {
    fn expression(&mut self, minimum: u8, depth: usize) -> Result<u64, CalculationError> {
        if depth >= MAX_EXPRESSION_DEPTH {
            return Err(CalculationError::NestingLimit);
        }
        let mut value = self.prefix(depth)?;
        loop {
            let rest = self.rest();
            let Some(&(token, operator, precedence)) =
                INFIX.iter().find(|(token, _, _)| rest.starts_with(token))
            else {
                break;
            };
            if precedence < minimum {
                break;
            }
            let operator_offset = self.index;
            self.index += token.len();
            let operand = self.expression(precedence + 1, depth + 1)?;
            let at = match self.arithmetic {
                Arithmetic::Wrapping => self.index,
                Arithmetic::Checked => operator_offset,
            };
            value = operator.apply(value, operand, at, self.arithmetic)?;
        }
        Ok(value)
    }

    fn prefix(&mut self, depth: usize) -> Result<u64, CalculationError> {
        match self.rest().as_bytes().first().copied() {
            Some(b'(') => {
                self.index += 1;
                let value = self.expression(1, depth + 1)?;
                if !self.rest().starts_with(')') {
                    return Err(CalculationError::MissingBracket { at: self.index });
                }
                self.index += 1;
                Ok(value)
            }
            Some(operator @ (b'+' | b'-' | b'!' | b'~')) => {
                let at = self.index;
                self.index += 1;
                let value = self.expression(11, depth + 1)?;
                match operator {
                    b'-' if self.arithmetic == Arithmetic::Checked => (value as i64)
                        .checked_neg()
                        .map(|result| result as u64)
                        .ok_or(CalculationError::ArithmeticOverflow { at }),
                    b'-' => Ok(value.wrapping_neg()),
                    b'!' => Ok(u64::from(value == 0)),
                    b'~' => Ok(!value),
                    _ => Ok(value),
                }
            }
            Some(b'@') => self.read_value(),
            Some(b'"') => self.quoted(),
            Some(first) if first.is_ascii_hexdigit() => self.number(),
            Some(_) => Err(CalculationError::InvalidOperator { at: self.index }),
            None => Err(self.expected("an operand")),
        }
    }

    fn rest(&mut self) -> &str {
        self.index += self.input.as_bytes()[self.index..]
            .iter()
            .take_while(|byte| byte.is_ascii_whitespace())
            .count();
        &self.input[self.index..]
    }

    fn expected(&self, expected: &'static str) -> CalculationError {
        CalculationError::Syntax {
            at: self.index,
            expected,
        }
    }

    fn number(&mut self) -> Result<u64, CalculationError> {
        let at = self.index;
        let bytes = self.input.as_bytes();
        let has_hex_prefix = bytes[at..].starts_with(b"0x") || bytes[at..].starts_with(b"0X");
        let start = at + if has_hex_prefix { 2 } else { 0 };
        self.index = start
            + bytes[start..]
                .iter()
                .take_while(|byte| byte.is_ascii_hexdigit())
                .count();
        let digits = &self.input[start..self.index];
        let suffix = bytes.get(self.index).map(u8::to_ascii_lowercase);
        let base = match suffix {
            Some(b'i') if !has_hex_prefix => 2,
            Some(b'o') if !has_hex_prefix => 8,
            Some(b't') if !has_hex_prefix => 10,
            Some(b'h') if !has_hex_prefix => 16,
            _ => 16,
        };
        if !has_hex_prefix && matches!(suffix, Some(b'i' | b'o' | b't' | b'h')) {
            self.index += 1;
        }
        u64::from_str_radix(digits, base).map_err(|error| {
            if *error.kind() == std::num::IntErrorKind::PosOverflow {
                CalculationError::BigNumber { at }
            } else {
                CalculationError::InvalidNumber { at }
            }
        })
    }

    fn quoted(&mut self) -> Result<u64, CalculationError> {
        let at = self.index;
        self.index += 1;
        let tail = &self.input[self.index..];
        let Some((string, _)) = tail.split_once('"') else {
            return Err(self.expected("a closing quote"));
        };
        if string.len() > 8 {
            return Err(CalculationError::BigNumber { at });
        }
        if !string.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
            return Err(self.expected("up to eight printable ASCII bytes"));
        }
        self.index += string.len() + 1;
        Ok(string
            .bytes()
            .fold(0, |word, byte| (word << 8) | u64::from(byte)))
    }

    fn read_value(&mut self) -> Result<u64, CalculationError> {
        let at = self.index;
        self.index += 1;
        let kind = self.input.as_bytes().get(self.index).copied();
        let width = match kind {
            Some(b'o' | b'O') => {
                self.index += 1;
                if self.input.as_bytes().get(self.index) == Some(&b':') {
                    return Err(CalculationError::InvalidByteOrder { at: self.index });
                }
                return if kind == Some(b'o') {
                    self.local
                        .ok_or(CalculationError::LocalOffsetOverflow { at })
                } else {
                    Ok(self.cursor.get())
                };
            }
            Some(b'b' | b'B') => 1,
            Some(b'w' | b'W') => 2,
            Some(b'd' | b'D') => 4,
            Some(b'q' | b'Q') => 8,
            _ => return Err(CalculationError::InvalidTag { at }),
        };
        self.index += 1;
        let order = self.read_byte_order()?;
        let mut buffer = [0; 8];
        let bytes = match order {
            Endian::Little => &mut buffer[..width],
            Endian::Big => &mut buffer[8 - width..],
        };
        if self
            .document
            .read_at(self.cursor, bytes)
            .map_err(CalculationError::Read)?
            != width
        {
            return Err(CalculationError::ShortRead {
                at,
                offset: self.cursor,
                width,
            });
        }
        let value = match order {
            Endian::Little => u64::from_le_bytes(buffer),
            Endian::Big => u64::from_be_bytes(buffer),
        };
        if kind.is_some_and(|kind| kind.is_ascii_lowercase()) {
            let shift = 64 - 8 * width;
            Ok((((value << shift) as i64) >> shift) as u64)
        } else {
            Ok(value)
        }
    }

    fn read_byte_order(&mut self) -> Result<Endian, CalculationError> {
        if self.input.as_bytes().get(self.index) != Some(&b':') {
            return Ok(Endian::Little);
        }
        let at = self.index;
        self.index += 1;
        let start = self.index;
        self.index += self.input.as_bytes()[start..]
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
            .count();
        let token = &self.input[start..self.index];
        let order = if token.eq_ignore_ascii_case("le") {
            Endian::Little
        } else if token.eq_ignore_ascii_case("be") {
            Endian::Big
        } else {
            return Err(CalculationError::InvalidByteOrder { at });
        };
        if self.input.as_bytes().get(self.index) == Some(&b':') {
            return Err(CalculationError::InvalidByteOrder { at: self.index });
        }
        Ok(order)
    }
}

#[derive(Clone, Copy)]
enum Operator {
    LogicalOr,
    LogicalAnd,
    BitOr,
    BitXor,
    BitAnd,
    Equal,
    Unequal,
    LessEqual,
    GreaterEqual,
    Less,
    Greater,
    Left,
    Right,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

// Longer spellings precede their single-character prefixes.
const INFIX: &[(&str, Operator, u8)] = &[
    ("||", Operator::LogicalOr, 1),
    ("&&", Operator::LogicalAnd, 2),
    ("==", Operator::Equal, 6),
    ("!=", Operator::Unequal, 6),
    ("<=", Operator::LessEqual, 7),
    (">=", Operator::GreaterEqual, 7),
    ("<<", Operator::Left, 8),
    (">>", Operator::Right, 8),
    ("|", Operator::BitOr, 3),
    ("^", Operator::BitXor, 4),
    ("&", Operator::BitAnd, 5),
    ("<", Operator::Less, 7),
    (">", Operator::Greater, 7),
    ("+", Operator::Add, 9),
    ("-", Operator::Subtract, 9),
    ("*", Operator::Multiply, 10),
    ("/", Operator::Divide, 10),
    ("%", Operator::Remainder, 10),
];

impl Operator {
    fn apply(
        self,
        left: u64,
        right: u64,
        at: usize,
        arithmetic: Arithmetic,
    ) -> Result<u64, CalculationError> {
        use Operator::*;
        let signed_left = left as i64;
        let signed_right = right as i64;
        if matches!(self, Divide | Remainder) && right == 0 {
            return Err(CalculationError::DivisionByZero { at });
        }
        let checked = arithmetic == Arithmetic::Checked;
        let require_result = |result: Option<i64>| {
            result
                .map(|value| value as u64)
                .ok_or(CalculationError::ArithmeticOverflow { at })
        };
        Ok(match self {
            LogicalOr => u64::from(left != 0 || right != 0),
            LogicalAnd => u64::from(left != 0 && right != 0),
            BitOr => left | right,
            BitXor => left ^ right,
            BitAnd => left & right,
            Equal => u64::from(left == right),
            Unequal => u64::from(left != right),
            LessEqual => u64::from(signed_left <= signed_right),
            GreaterEqual => u64::from(signed_left >= signed_right),
            Less => u64::from(signed_left < signed_right),
            Greater => u64::from(signed_left > signed_right),
            Left if right < 64 => left << right,
            Right if right < 64 => left >> right,
            Left | Right if checked => return Err(CalculationError::InvalidShiftCount { at }),
            Left | Right => 0,
            Add if checked => return require_result(signed_left.checked_add(signed_right)),
            Subtract if checked => return require_result(signed_left.checked_sub(signed_right)),
            Multiply if checked => return require_result(signed_left.checked_mul(signed_right)),
            Divide if checked => return require_result(signed_left.checked_div(signed_right)),
            Remainder if checked => return require_result(signed_left.checked_rem(signed_right)),
            Add => left.wrapping_add(right),
            Subtract => left.wrapping_sub(right),
            Multiply => left.wrapping_mul(right),
            Divide => signed_left.wrapping_div(signed_right) as u64,
            Remainder => signed_left.wrapping_rem(signed_right) as u64,
        })
    }
}

#[cfg(test)]
#[path = "calculator_tests.rs"]
mod tests;
