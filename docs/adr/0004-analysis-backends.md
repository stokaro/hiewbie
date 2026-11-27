# ADR 0004: keep analysis selection open until capability experiments pass

Status: decision gate, not an accepted backend selection. This explicitly keeps
M0's analysis feasibility work open. The raw editor introduces no executable
parser, decoder, assembler, native engine, or proprietary reference material.

## Constraints

The complete [format and instruction contract](../compatibility.md) and
[host matrix](../build-and-release.md) remain required. A backend's advertised
instruction coverage is not equivalent to a hiewbie capability. Decoding,
textual assembly, address mapping, branch/reference analysis, and browser
availability must be checked independently.

## Next experiments

| Boundary | Candidate experiment | Acceptance before selection |
| --- | --- | --- |
| Executable formats | Pin and evaluate Goblin or focused readers against generated PE/ELF/Mach-O fixtures | Bounded malformed-input handling; explicit RVA/VA/file/slice mapping; raw fallback; legacy-format gaps recorded |
| x86 decode/encode | Pin iced-x86 and exercise 16/32/64-bit fixtures and relative instructions | Exact bytes/addresses; host-independent decoding; wasm compilation and actual browser feasibility tracked |
| ARM families | Compare a pinned permissive Rust decoder with Capstone | ARMv6/Thumb and additive ARM64 coverage; malformed input; all native target build requirements; wasm route |
| Text assembly | Evaluate a text parser plus encoder or pinned LLVM MC integration | User-entered syntax, current address/mode/endian context, preview, length rejection, transaction atomicity, licensing and full packaging matrix |

The `object` crate's documented trusted-input scope requires an explicit
arbitrary-input assessment before selection. iced-x86's encoder/CodeAssembler
API is not itself a textual assembly parser. Keystone's GPLv2/FOSS-exception
terms must not be described as MIT; no Keystone code or dependency is added.
The sources for these candidate constraints are in
[references-and-reuse.md](../references-and-reuse.md).

Each experiment must record the exact revision, license/notices, fixture
provenance, limits, target compilation, and runtime evidence. Unavailable
capabilities stay visible; adding one backend must not silently drop old HIEW
formats or supported host targets. A separate implementation PR should add
checked address-domain types and the first tested parser/decoder together,
without selecting an assembler merely to fill a checkbox.
