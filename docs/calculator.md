# Calculator expression contract

Requirements: **CALC-001**, **CALC-002**. Reference scenario: **H23**; UX row:
**UX-ADV-02**. This contract describes the shared evaluator used by the native
Calculator and `Editor::calculate` / `Editor::calculate_checked`.

## Evidence and compatibility

The baseline is HIEW **8.63**. The existing
[Windows observation](evidence/hiew-8.63-windows-calculator.json) records paired
dialog, data-read, address, error and pending-Hex-input checks. The accompanying
[reference notes](reference/hiew-8.63.md#calculator) establish wrapping arithmetic,
signed division/comparison, logical right shift, mixed-base input and signed vs
unsigned cursor tags. That observation is not a per-expression transcript of
every operator combination.

The [independent known-answer corpus](../crates/hiewbie-core/src/calculator_tests.rs)
specifies the implemented grammar and boundaries below. Its constants come from
integer arithmetic and explicit byte fixtures; passing it does not invent new
original-runtime observations. Checked evaluation and the `:le` / `:be` suffixes
are hiewbie additions. Broader differential reference capture remains tracked in
[#24](https://github.com/stokaro/hiewbie/issues/24).

## Values and number bases

Every result is a 64-bit bit pattern, shown as signed decimal, unsigned decimal,
binary, split hexadecimal and eight display bytes. Integer literals may use all
64 bits, including in checked mode. A bit pattern with its high bit set is
negative for signed operations; checked mode does not redefine it as a positive
arbitrary-precision integer.

| Form | Meaning | Example result |
| --- | --- | --- |
| Bare digits | Hexadecimal, including leading zeroes | `010` = `10h` |
| `0x` / `0X` prefix, or `h` / `H` suffix | Explicit hexadecimal | `0x10` = `10h` |
| `i` / `I` suffix | Binary | `10i` = `2h` |
| `o` / `O` suffix | Octal | `10o` = `8h` |
| `t` / `T` suffix | Decimal | `10t` = `Ah` |
| Double-quoted printable ASCII | Up to eight bytes packed high-byte first | `"AB"` = `4142h` |

Prefixes and suffixes are contiguous with their digits. Use a prefix or a suffix,
not both on one literal. In particular, `10b` is hexadecimal `10Bh`; binary uses
`i`. An empty quoted string evaluates to zero. Quoted bytes have no escape syntax
and their order is independent of typed-read byte order. Invalid digits, literals
larger than 64 bits and non-ASCII quoted text produce explicit errors.

## Operators and evaluation order

Higher rows bind more tightly. Parentheses override this order. Binary operators
at the same level associate left to right; prefix operators nest right to left.

| Priority | Operators | Meaning |
| --- | --- | --- |
| 11 | `+ - ! ~` (prefix) | Identity, negation, logical NOT, bitwise complement |
| 10 | `* / %` | Multiplication, signed division, signed remainder |
| 9 | `+ -` | Addition, subtraction |
| 8 | `<< >>` | Bit-pattern shifts; right shift fills with zero |
| 7 | `< <= > >=` | Signed comparisons |
| 6 | `== !=` | Bit-pattern equality and inequality |
| 5 | `&` | Bitwise AND |
| 4 | `^` | Bitwise XOR |
| 3 | `\|` | Bitwise OR |
| 2 | `&&` | Logical AND |
| 1 | `\|\|` | Logical OR |

Comparisons and logical operators return zero or one. Logical operators evaluate
both operands: `0&&(1/0)` reports division by zero. Division truncates toward zero;
the remainder has the dividend's sign. ASCII whitespace may separate tokens.
There are no assignments, implicit multiplication or document-write operators.

| Behavior | Enter / `calculate` | Ctrl+E / `calculate_checked` |
| --- | --- | --- |
| Addition, subtraction, multiplication, negation | Wrap modulo 2^64 | Reject signed 64-bit overflow |
| `8000000000000000/-1` | `8000000000000000h` | Overflow error |
| `8000000000000000%-1` | Zero | Overflow error |
| Division or remainder by zero | Error | Error |
| Shift count 0 through 63 | Shift the bit pattern | Same; discarded bits are allowed |
| Negative shift count or count at least 64 | Zero | Invalid-shift error |

For example, `1<<3F` is valid in both modes; checked shifts validate the count,
not signed arithmetic multiplication. The checked diagnostic identifies the
operator that failed. The Calculator title marks the last checked evaluation
with `[checked i64]`, including an error. Enter returns the title to the legacy
Calculator label. Editing or clearing the input retains the previous result and
its evaluation-mode label until another evaluation.

## Typed reads and addresses

All value tags read the current logical bytes at the file cursor. Reads need not
be aligned. Lowercase selects signed interpretation; uppercase selects unsigned.
Signed values narrower than 64 bits are sign-extended after byte-order decoding.

| Width | Signed tag | Unsigned tag | Default order |
| --- | --- | --- | --- |
| 8 bits / 1 byte | `@b` | `@B` | Little endian |
| 16 bits / 2 bytes | `@w` | `@W` | Little endian |
| 32 bits / 4 bytes | `@d` | `@D` | Little endian |
| 64 bits / 8 bytes | `@q` | `@Q` | Little endian |

Append `:le` or `:be` to any value tag to select little or big endian explicitly.
These suffixes are case-insensitive and contiguous with the tag. The expression
shows the effective width, signedness and byte order, so different read orders
can coexist in one calculation without changing a global setting.

For bytes `80 FE`, `@W:be` is `80FEh` and `@w:be` is -32514; `@W:le`
is `FE80h` and `@w:le` is -384. `@W:be+@W:le` reads the same two bytes
twice in their respective orders. One-byte order qualifiers are accepted and
have the same value. The default is always little endian, independently of the
executable's byte order or the host CPU.

`@o` returns the current local address from the executable/raw interpretation;
`@O` returns the file offset. They are addresses, not memory reads, and reject
byte-order suffixes. Where executable mapping has no usable address, `@o` keeps
the existing file-offset fallback. An overflowing raw-base address fails
explicitly. Unknown width tags and missing/unsupported orders also fail
explicitly; an invalid read qualifier is rejected before issuing that read.

## State and work bounds

Evaluation never writes bytes, creates an undo step, moves the cursor or changes
search continuation. Completed pending edits are visible to reads. An unfinished
Hex nibble remains in the frontend until the second digit completes the byte.
Help, close/reopen, errors and resizing preserve the expression and edit context.
Read-only documents support the same evaluations.

Expressions are limited to 512 bytes and recursive parsing to 64 levels. Each
typed read requests at most eight bytes and must return the entire width; EOF,
truncation and underlying I/O errors produce diagnostics. Offsets never wrap.
No expression requires reading the entire file. Local-address resolution may
use the existing bounded executable-metadata reader.

Verification includes the core corpus and faulting sources,
[terminal cell/state tests](../crates/hiewbie/src/terminal/tests.rs), and the
[PTY calculator scenario](../scripts/pty_smoke.py) with a generated fixture,
unchanged source bytes/mtime and terminal cleanup. The
[recording](evidence/calculator.cast) demonstrates wrapping, checked arithmetic,
mixed read orders, errors, Help and restoration of pending Hex input.
