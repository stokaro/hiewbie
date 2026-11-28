"""Compare fixed assembly fixtures with an installed LLVM MC toolchain."""

from pathlib import Path
from subprocess import PIPE, run
from tempfile import TemporaryDirectory
import argparse


def probe(llvm_bin: Path) -> None:
    mc = str(llvm_bin / "llvm-mc")
    objcopy = str(llvm_bin / "llvm-objcopy")
    readobj = str(llvm_bin / "llvm-readobj")
    print(run([mc, "--version"], check=True, stdout=PIPE, text=True).stdout.splitlines()[0], flush=True)

    with TemporaryDirectory(prefix="hiewbie-assembler-llvm-") as directory:
        object_file = Path(directory, "probe.o")
        section = Path(directory, "probe.bin")

        def assemble(triple: str, source: str):
            return run(
                [mc, f"--triple={triple}", "--filetype=obj", "-o", str(object_file)],
                input=source,
                stdout=PIPE,
                stderr=PIPE,
                text=True,
            )

        def expect(triple: str, source: str, expected: bytes, label: str) -> None:
            result = assemble(triple, source)
            if result.returncode != 0:
                raise RuntimeError(result.stderr)
            run(
                [objcopy, "-O", "binary", "--only-section=.text", str(object_file), str(section)],
                check=True,
            )
            actual = section.read_bytes()[: len(expected)]
            assert actual == expected, (label, actual.hex(), expected.hex())
            print(f"{label}: {actual.hex().upper()}")

        intel = ".intel_syntax noprefix\n"
        expect("i386-unknown-linux-gnu", intel + ".code16\nmov ax, 0xa\n", bytes.fromhex("B8 0A 00"), "x86-16 MOV")
        expect("i386-unknown-linux-gnu", intel + "mov eax, 0x12345678\n", bytes.fromhex("B8 78 56 34 12"), "x86-32 MOV")
        expect("x86_64-unknown-linux-gnu", intel + "vaddps xmm1,xmm2,xmm3\n", bytes.fromhex("C5 E8 58 CB"), "x86-64 AVX")
        for triple, prefix, mnemonic, pc, expected in [
            ("x86_64-unknown-linux-gnu", intel, "jmp", 0x1000, "EB 0E"),
            ("x86_64-unknown-linux-gnu", intel, "jmp", 0x1001, "EB 0D"),
            ("aarch64-unknown-linux-gnu", "", "b", 0x1000, "04 00 00 14"),
            ("aarch64-unknown-linux-gnu", "", "b", 0x1004, "03 00 00 14"),
        ]:
            distance = 0x1010 - pc
            target = f"\n.org {distance}\ndestination:\nnop\n"
            expect(triple, prefix + mnemonic + " destination" + target, bytes.fromhex(expected), f"{triple} @{pc:04X}")

        result = assemble("x86_64-unknown-linux-gnu", intel + "jmp unresolved_label\n")
        if result.returncode != 0:
            raise RuntimeError(result.stderr)
        relocations = run([readobj, "--relocations", str(object_file)], check=True, stdout=PIPE, text=True)
        assert "unresolved_label" in relocations.stdout
        print("unresolved label: object assembly succeeds with an unresolved relocation")

        result = assemble("aarch64-unknown-linux-gnu", "b #0x8000000\n")
        assert result.returncode != 0
        print("AArch64 out-of-range branch: rejected")

        result = assemble("i386-unknown-linux-gnu", intel + ".code16\nmov ax, 10t\n")
        print(f"literal 10t: LLVM MC returned {result.returncode}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("llvm_bin", type=Path, help="Directory containing llvm-mc, llvm-objcopy, and llvm-readobj")
    probe(parser.parse_args().llvm_bin)
