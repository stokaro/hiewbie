use super::*;

fn encoded(source: &str, pc: u64) -> Result<Vec<Vec<u8>>, AssemblyError> {
    assemble(
        source,
        DecodeMode::Aarch64,
        Endian::Little,
        VirtualAddress::new(pc),
        |_| None,
    )
}

#[test]
fn aarch64_assembly_reproduces_independent_llvm_words() {
    for row in include_str!("../../../../tests/fixtures/aarch64-assembly.tsv").lines() {
        if row.starts_with('#') {
            continue;
        }
        let [pc, bytes, source, _]: [&str; 4] =
            row.split('\t').collect::<Vec<_>>().try_into().unwrap();
        let pc = u64::from_str_radix(pc, 16).unwrap();
        let expected = u32::from_str_radix(bytes, 16).unwrap().to_be_bytes();
        assert_eq!(encoded(source, pc), Ok(vec![expected.to_vec()]), "{row}");
    }
}

#[test]
fn aarch64_names_resolve_at_distinct_addresses_and_decode_back() {
    let target = VirtualAddress::new(0x0001_4000_4234);
    let code = |source, pc| {
        assemble(
            source,
            DecodeMode::Aarch64,
            Endian::Little,
            VirtualAddress::new(pc),
            |name| (name == "PageTarget").then_some(target),
        )
    };
    for (pc, bytes) in [
        (0x0001_4000_1ff8, [0x01, 0x00, 0x00, 0xf0]),
        (0x0001_4000_2004, [0x01, 0x00, 0x00, 0xd0]),
    ] {
        let output = code("ADRP X1, @PageTarget", pc).unwrap();
        assert_eq!(output, vec![bytes.to_vec()]);
        let row = crate::instructions::decode_one(
            &output[0],
            VirtualAddress::new(pc),
            DecodeMode::Aarch64,
            Endian::Little,
        )
        .unwrap();
        assert_eq!(
            row.address_operand,
            Some(crate::instructions::AddressOperand::Literal(
                VirtualAddress::new(0x0001_4000_4000)
            ))
        );
    }
    assert_eq!(
        code("ldr q9, @PageTarget-4", 0x0001_4000_4224),
        Ok(vec![vec![0x69, 0, 0, 0x9c]])
    );
    assert_eq!(
        code("adrp x1, @pagetarget", 0x1000),
        Err(AssemblyError::UnresolvedSymbol("pagetarget".into()))
    );
}

#[test]
fn aarch64_assembly_diagnoses_address_register_and_shape_boundaries() {
    for (source, pc) in [
        ("adr x0, 201000", 0x1000), // no ADRP+ADD expansion
        ("adr x0, 0", 0x100004),
        ("adrp x0, 0", 0x1_0000_1000),
        ("adrp x0, 0x100000000", 0),
        ("b FFFFFFFFFFFFFFFC", 0),
        ("b 0", u64::MAX - 3),
        ("b 2001", 0x1000),
        ("ldr x0, 1001", 0x1000),
        ("ldr q9, 101000", 0x1000),
        ("ldr q9, 0", 0x100004),
        ("nop", 0x1001),
        ("tbz w0, #20, 1000", 0x1000),
        ("movk w0, #1, lsl #20", 0x1000),
        ("stp q1, q2, [sp, #8]", 0x1000),
        ("ldraa x1, [x2, #7]", 0x1000),
        ("ld1d {z1.d}, p0/z, [x0, #8, mul vl]", 0x1000),
    ] {
        assert!(
            matches!(encoded(source, pc), Err(AssemblyError::OutOfRange(_))),
            "{source}: {:?}",
            encoded(source, pc)
        );
    }
    assert_eq!(
        encoded("ldr x0, FFFFFFFFFFFFFFF8", u64::MAX - 3),
        Ok(vec![vec![0xe0, 0xff, 0xff, 0x58]])
    );
    assert_eq!(
        encoded("adr x0, FFFFFFFFFFFFFFFF", u64::MAX - 3),
        Ok(vec![vec![0x00, 0x00, 0x00, 0x70]])
    );
    for source in [
        "ldr w32, 1008",
        "ldrsw w1, 1008",
        "ldr b0, 1008",
        "adr w0, 1000",
        "ldp x0, w1, [x3]",
        "ldr xzr, [xzr]",
        "ldr x1, [x1, #8]!",
        "ldur x1, [x2, x3]",
        "ldr x1, [x2, w3]",
        "ldr x1, [x2, #8], #8",
        "mov xzr, sp",
        "add x0, x1, w2",
        "fadd d0, s1, d2",
        "ld1w {z0.d}, p1/z, [x3]",
        "ld1w {z0.s}, p8/z, [x3]",
        "ld1w {z0.s}, p1/m, [x3]",
        "eor3 z0.d, z1.d, z2.d, z3.d",
        "sqxtnb z1.s, z2.h",
        "prfm bogus, 1008",
        "msr cntvct_el0, x0",
        "smopa za0.s, p0/m, p0/m, z0.s, z0.s",
        "mov x0, #0x1122334455667788",
        "and x0, x1, #0x123456",
        ".word 0",
        "nop\nret",
        "nop: nop",
        "add x0,, x2",
        "ldr x1, [[x2]]",
    ] {
        assert!(encoded(source, 0x1000).is_err(), "{source}");
    }
    assert_eq!(
        encoded(&" ".repeat(MAX_SOURCE_BYTES + 1), 0),
        Err(AssemblyError::SourceLimit)
    );
    assert_eq!(
        assemble(
            "nop",
            DecodeMode::Aarch64,
            Endian::Big,
            VirtualAddress::new(0),
            |_| None
        ),
        Err(AssemblyError::UnsupportedProfile)
    );
}
