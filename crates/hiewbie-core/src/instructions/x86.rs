use iced_x86::{
    Decoder, DecoderError, DecoderOptions, Formatter, FormatterOutput, FormatterTextKind,
    IntelFormatter, OpKind,
};

use super::{
    BranchTarget, DecodeMode, DecodeStatus, DecodedInstruction, TextBuffer, fallback, instruction,
};
use crate::formats::VirtualAddress;

impl FormatterOutput for TextBuffer {
    fn write(&mut self, text: &str, _kind: FormatterTextKind) {
        self.append(text);
    }
}

pub(super) fn decode(
    bytes: &[u8],
    address: VirtualAddress,
    mode: DecodeMode,
) -> DecodedInstruction {
    // The mode enum constrains the bitness to the decoder's three supported values.
    let mut decoder = Decoder::with_ip(mode.bitness(), bytes, address.get(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    if decoded.is_invalid() || decoded.len() == 0 || decoded.len() > bytes.len() {
        let status = if decoder.last_error() == DecoderError::NoMoreBytes {
            DecodeStatus::Truncated
        } else {
            DecodeStatus::Unknown
        };
        return fallback(bytes, address, mode, status);
    }
    let direct_branch = decoded.op_kinds().find_map(|kind| {
        matches!(
            kind,
            OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
        )
        .then(|| BranchTarget {
            address: VirtualAddress::new(decoded.near_branch_target()),
            mode,
        })
    });
    let mut formatter = IntelFormatter::new();
    formatter.options_mut().set_hex_prefix("0x");
    formatter.options_mut().set_hex_suffix("");
    formatter.options_mut().set_uppercase_hex(false);
    formatter
        .options_mut()
        .set_small_hex_numbers_in_decimal(false);
    let mut text = TextBuffer::new();
    formatter.format(&decoded, &mut text);
    instruction(
        &bytes[..decoded.len()],
        address,
        text.finish(),
        DecodeStatus::Decoded,
        direct_branch,
    )
}
