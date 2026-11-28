use hiewbie_assembler_probe::{CASES, assemble, verify};

#[cfg(feature = "compare-asm-rs")]
use hiewbie_assembler_probe::Profile;

fn bytes(items: &[u8]) -> String {
    items.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn main() {
    assert!(
        verify(),
        "the independent prototype disagrees with the golden bytes"
    );
    for case in CASES {
        let input = case.input;
        let pc = case.pc;
        let profile = case.profile;
        let result = assemble(input, profile, pc)
            .map(|items| {
                items
                    .iter()
                    .map(|item| bytes(item))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|error| format!("reject ({error})"));
        println!("{profile:?} @{pc:04X} {input} -> {result}");

        #[cfg(feature = "compare-asm-rs")]
        {
            let (arch, source) = match profile {
                Profile::X86_16 => (asm_rs::Arch::X86, format!(".code16\n{input}")),
                Profile::X86_32 => (asm_rs::Arch::X86, input.to_owned()),
                Profile::X86_64 => (asm_rs::Arch::X86_64, input.to_owned()),
                Profile::Aarch64 => (asm_rs::Arch::Aarch64, input.to_owned()),
            };
            match asm_rs::assemble_at(&source, arch, pc) {
                Ok(result) => println!("  asm-rs -> {}", bytes(&result)),
                Err(error) => println!("  asm-rs -> reject ({error})"),
            }
        }
    }

    #[cfg(feature = "compare-asm-rs")]
    {
        println!("asm-rs using its own notation and external absolute labels");
        for (arch, pc, source, target) in [
            (asm_rs::Arch::X86, 0x1000, ".code16\nmov ax, 10", None),
            (asm_rs::Arch::X86, 0x1000, "mov eax, 0x12345678", None),
            (
                asm_rs::Arch::X86_64,
                0x1000,
                "jmp destination",
                Some(0x1010),
            ),
            (
                asm_rs::Arch::X86_64,
                0x1001,
                "jmp destination",
                Some(0x1010),
            ),
            (asm_rs::Arch::Aarch64, 0x1000, "b destination", Some(0x1010)),
            (asm_rs::Arch::Aarch64, 0x1004, "b destination", Some(0x1010)),
            (
                asm_rs::Arch::Aarch64,
                0x1000,
                "b destination",
                Some(0x8001000),
            ),
        ] {
            let result = match target {
                Some(target) => asm_rs::assemble_with(source, arch, pc, &[("destination", target)]),
                None => asm_rs::assemble_at(source, arch, pc),
            };
            let result = result
                .map(|result| bytes(&result))
                .unwrap_or_else(|error| format!("reject ({error})"));
            println!("  {arch:?} @{pc:04X} {source:?} target {target:X?} -> {result}");
        }
    }
}
