use iced_x86::{Decoder, DecoderOptions, Mnemonic};

use super::*;
use DecodeMode::{X86_16, X86_32, X86_64};

fn at(source: &str, mode: DecodeMode, address: u64) -> Result<Vec<Vec<u8>>, AssemblyError> {
    assemble_x86(
        source,
        mode,
        Endian::Little,
        VirtualAddress::new(address),
        |_| None,
    )
}

fn bytes(text: &str) -> Vec<u8> {
    text.split_ascii_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

#[test]
fn textual_forms_encode_independently_known_bytes() {
    for (mode, source, expected) in [
        (X86_16, "mov ax,10t", "B8 0A 00"),
        (X86_16, "mov bx,[bp+si-8]", "8B 5A F8"),
        (X86_16, "mov cx,[123+23-46h]", "8B 0E 00 01"),
        (X86_32, "mov eax,12345678", "B8 78 56 34 12"),
        (X86_32, "MOV eax,\"sign\"", "B8 73 69 67 6E"),
        (X86_32, "mov eax,\"a,+;\" ; comment", "B8 61 2C 2B 3B"),
        (X86_32, "cmp eax,80", "3D 80 00 00 00"),
        (X86_32, "cmp eax,0FFFFFFFFh", "83 F8 FF"),
        (X86_32, "jmpf f,[eax]", "FF 28"),
        (X86_32, "jmpf d,[eax]", "66 FF 28"),
        (X86_32, "callf f,[eax]", "FF 18"),
        (X86_16, "lock add w,[bx],1", "F0 83 07 01"),
        (X86_32, "jmp 1234:12345678", "EA 78 56 34 12 34 12"),
        (X86_32, "fadd st(0),st(1)", "D8 C1"),
        (X86_32, "paddb mm1,mm2", "0F FC CA"),
        (X86_32, "addps xmm1,xmm2", "0F 58 CA"),
        (X86_32, "aesenc xmm1,xmm2", "66 0F 38 DC CA"),
        (X86_32, "pclmulqdq xmm1,xmm2,0", "66 0F 3A 44 CA 00"),
        (X86_32, "popcnt eax,ecx", "F3 0F B8 C1"),
        (X86_32, "vaddps xmm1,xmm2,xmm3", "C5 E8 58 CB"),
        (X86_32, "vaddps ymm1,ymm2,ymm3", "C5 EC 58 CB"),
        (X86_32, "enter 10,0", "C8 10 00 00"),
        (X86_64, "mov eax,DWORD PTR [rbx - 8]", "8B 43 F8"),
        (X86_64, "lea rax,[rbx+rcx*4-8]", "48 8D 44 8B F8"),
        (
            X86_64,
            "mov rax,11223344`55667788",
            "48 B8 88 77 66 55 44 33 22 11",
        ),
        (X86_64, "mov eax,[rip - 4]", "8B 05 FC FF FF FF"),
        (X86_64, "add rax,0FFFFFFFFFFFFFFFFh", "48 83 C0 FF"),
        (X86_64, "rep movsb", "F3 A4"),
        (X86_64, "lock add dword [rcx],1", "F0 83 01 01"),
        (X86_64, "vblendps xmm1,xmm2,xmm3,5", "C4 E3 69 0C CB 05"),
    ] {
        let actual = at(source, mode, 0x1000).unwrap_or_else(|error| panic!("{source}: {error}"));
        assert_eq!(
            actual[0],
            bytes(expected),
            "{mode}: {source}. Candidates {actual:?}"
        );
    }
}

#[test]
fn distinct_legacy_encodings_are_stable() {
    let expected = [
        bytes("83 F8 FF"),
        bytes("3D FF FF FF FF"),
        bytes("81 F8 FF FF FF FF"),
    ];
    assert_eq!(at("cmp eax,FFFFFFFF", X86_32, 0).unwrap(), expected);
    assert_eq!(at("cmp eax,FFFFFFFF", X86_32, 0x1000).unwrap(), expected);
}

#[test]
fn near_and_short_branches_use_the_actual_address_in_every_mode() {
    for (mode, near, condition) in [
        (X86_16, "E9 0D 00", "0F 84 0C 00"),
        (X86_32, "E9 0B 00 00 00", "0F 84 0A 00 00 00"),
        (X86_64, "E9 0B 00 00 00", "0F 84 0A 00 00 00"),
    ] {
        assert_eq!(at("jmp 1010", mode, 0x1000).unwrap(), [bytes(near)]);
        assert_eq!(at("jmps 1010", mode, 0x1001).unwrap(), [bytes("EB 0D")]);
        assert_eq!(
            at("jz 1010", mode, 0x1000).unwrap(),
            [bytes("74 0E"), bytes(condition)]
        );
        assert_eq!(at("jmps 1081", mode, 0x1000).unwrap(), [bytes("EB 7F")]);
        assert!(matches!(
            at("jmps 1082", mode, 0x1000),
            Err(AssemblyError::OutOfRange(_))
        ));
        assert_eq!(at("jmps F82", mode, 0x1000).unwrap(), [bytes("EB 80")]);
        assert!(matches!(
            at("jmps F81", mode, 0x1000),
            Err(AssemblyError::OutOfRange(_))
        ));
    }
    assert_eq!(
        at("jmp 200010", X86_16, 0x200000).unwrap(),
        [bytes("E9 0D 00")]
    );
    assert!(matches!(
        at("jmp 200010", X86_16, 0),
        Err(AssemblyError::OutOfRange(_))
    ));
    assert!(matches!(
        at("jmp 0", X86_64, u64::MAX - 1),
        Err(AssemblyError::OutOfRange(_))
    ));
    assert!(matches!(
        at("jmp 0", X86_32, 0x100000000),
        Err(AssemblyError::OutOfRange(_))
    ));
}

#[test]
fn named_addresses_and_ambiguous_names_are_explicit() {
    let encode = |source| {
        assemble_x86(
            source,
            X86_64,
            Endian::Little,
            VirtualAddress::new(0x140001000),
            |name| match name {
                "entry" => Some(VirtualAddress::new(0x140001010)),
                "face" => Some(VirtualAddress::new(0x14000100A)),
                _ => None,
            },
        )
    };
    assert_eq!(encode("jmp entry+1").unwrap(), [bytes("E9 0C 00 00 00")]);
    assert_eq!(encode("jmps @face").unwrap(), [bytes("EB 08")]);
    assert_eq!(
        encode("jmp Entry"),
        Err(AssemblyError::UnresolvedSymbol("Entry".into()))
    );
}

#[test]
fn nop_variants_match_the_observed_lengths_and_decode_as_one_instruction_each() {
    for (mode, count) in [(X86_16, 1), (X86_32, 9), (X86_64, 9)] {
        let choices = at("nop", mode, 0).unwrap();
        assert_eq!(choices.len(), count);
        assert_eq!(choices[0], bytes("90"));
        for (index, choice) in choices.iter().enumerate() {
            assert_eq!(choice.len(), index + 1);
            let decoded = Decoder::new(mode.bitness(), choice, DecoderOptions::NONE).decode();
            assert_eq!(decoded.len(), choice.len());
            assert_eq!(decoded.mnemonic(), Mnemonic::Nop);
        }
    }
    assert_eq!(at("nop", X86_64, u64::MAX - 1).unwrap(), [bytes("90")]);
}

#[test]
fn malformed_and_unsupported_sources_return_specific_errors() {
    assert_eq!(
        at("add [rax],1", X86_64, 0),
        Err(AssemblyError::AmbiguousMemorySize)
    );
    assert_eq!(
        at("badopcode", X86_64, 0),
        Err(AssemblyError::UnknownMnemonic("badopcode".into()))
    );
    assert!(matches!(
        at("mov eax,100000000", X86_32, 0),
        Err(AssemblyError::OutOfRange(_))
    ));
    assert!(matches!(
        at("vpxor ymm1,ymm2,ymm3", X86_64, 0),
        Err(AssemblyError::UnsupportedForm(_))
    ));
    assert!(matches!(
        at("vaddps xmm1,xmm2,xmm3", X86_16, 0),
        Err(AssemblyError::UnsupportedForm(_))
    ));
    assert!(matches!(
        at("vaddps zmm1,zmm2,zmm3", X86_64, 0),
        Err(AssemblyError::UnsupportedForm(_))
    ));
    for source in [
        "mov eax,,1",
        "mov [rax+],1",
        "mov [rax+rbx*3],1",
        "mov eax,--1",
        "mov eax,\"unmatched",
        "nop\nint3",
        "nop\r",
        "möv ax,12",
        "nop 1,2,3,4,5,6",
        "[rax+rbx",
        "mov eax,[",
        "nop\0",
    ] {
        assert!(at(source, X86_64, 0).is_err(), "{source}");
    }
    assert_eq!(
        at(&" ".repeat(MAX_SOURCE_BYTES + 1), X86_64, 0),
        Err(AssemblyError::SourceLimit)
    );
    assert_eq!(
        assemble_x86("nop", X86_32, Endian::Big, VirtualAddress::new(0), |_| None),
        Err(AssemblyError::UnsupportedProfile)
    );
    assert_eq!(
        at("nop", DecodeMode::Aarch64, 0),
        Err(AssemblyError::UnsupportedProfile)
    );
}
