use super::*;
use crate::{Command, Editor, MemorySource, SearchProgress, SearchRepeat, SearchRequest};
use std::io;

fn document(bytes: &[u8]) -> Document<MemorySource> {
    Document::new(MemorySource::new(bytes.to_vec()))
}

#[test]
fn hiew_arithmetic_literals_and_operator_precedence() {
    let mut data = document(&[]);
    for (expression, expected) in [
        ("1+2*3", 7),
        ("(1+2)*3", 9),
        ("010+100t+10i", 0x76),
        ("10o+0x10+10h", 0x28),
        ("\"AB\"+1", 0x4143),
        ("FFFFFFFFFFFFFFFF+1", 0),
        ("8000000000000000/-1", 1 << 63),
        ("8000000000000000%-1", 0),
        ("-3/2", u64::MAX),
        ("-3%2", u64::MAX),
        ("-1<0", 1),
        ("-1>>1", u64::MAX >> 1),
        ("1<<40", 0),
        ("1<<-1", 0),
        ("1|2&4", 1),
        ("2&6==2", 0),
        ("1==1<0", 0),
        ("!1", 0),
        ("~1", u64::MAX - 1),
    ] {
        assert_eq!(
            evaluate(expression, &mut data, FileOffset::new(0), Some(0)).unwrap(),
            expected,
            "{expression}"
        );
    }
}

#[test]
fn checked_arithmetic_reports_the_overflowing_operator() {
    let mut data = document(&[]);
    for (expression, at) in [
        ("7FFFFFFFFFFFFFFF + (1 * 1)", 17),
        ("8000000000000000-1", 16),
        ("4000000000000000*2", 16),
        ("-8000000000000000", 0),
        ("8000000000000000/-1", 16),
        ("8000000000000000%-1", 16),
    ] {
        assert!(
            matches!(
                evaluate_checked(expression, &mut data, FileOffset::new(0), None),
                Err(CalculationError::ArithmeticOverflow { at: offset }) if offset == at
            ),
            "{expression}"
        );
    }
    for expression in ["1<<40", "1>>-1"] {
        assert!(
            matches!(
                evaluate_checked(expression, &mut data, FileOffset::new(0), None),
                Err(CalculationError::InvalidShiftCount { at: 1 })
            ),
            "{expression}"
        );
    }
    assert!(matches!(
        evaluate_checked("1/0", &mut data, FileOffset::new(0), None),
        Err(CalculationError::DivisionByZero { at: 1 })
    ));
}

#[test]
fn checked_arithmetic_keeps_signed_values_and_bit_operations() {
    let mut data = document(&[0xFE]);
    for (expression, expected) in [
        ("@b+10i*3t", 4),
        ("FFFFFFFFFFFFFFFF+1", 0),
        ("8000000000000000+0", 1 << 63),
        ("1<<3F", 1 << 63),
        ("FFFFFFFFFFFFFFFF<<1", u64::MAX - 1),
        ("-1>>1", u64::MAX >> 1),
        ("-3%2", u64::MAX),
    ] {
        assert_eq!(
            evaluate_checked(expression, &mut data, FileOffset::new(0), None).unwrap(),
            expected,
            "{expression}"
        );
    }
}

#[test]
fn reads_little_endian_document_bytes_and_distinguishes_coordinates() {
    let mut data = document(&[0xFE, 0x80, 1, 2, 3, 4, 5, 0x86, 0x7F]);
    let origin = FileOffset::new(0);
    for (expression, expected) in [
        ("@b", (-2_i64) as u64),
        ("@B", 0xFE),
        ("@w", (-0x7F02_i64) as u64),
        ("@W", 0x80FE),
        ("@D", 0x0201_80FE),
        ("@Q", 0x8605_0403_0201_80FE),
        ("@o", 0x1_4000_1000),
        ("@O", 0),
    ] {
        assert_eq!(
            evaluate(expression, &mut data, origin, Some(0x1_4000_1000)).unwrap(),
            expected,
            "{expression}"
        );
    }
    assert!(matches!(
        evaluate("@Q", &mut data, FileOffset::new(2), None),
        Err(CalculationError::ShortRead {
            at: 0,
            width: 8,
            ..
        })
    ));
    assert!(matches!(
        evaluate("@o", &mut data, origin, None),
        Err(CalculationError::LocalOffsetOverflow { at: 0 })
    ));
}

#[test]
fn reports_invalid_input_without_unbounded_recursion() {
    let mut data = document(&[]);
    let mut evaluate =
        |expression: &str| evaluate(expression, &mut data, FileOffset::new(0), Some(0));
    assert!(matches!(
        evaluate("1/0"),
        Err(CalculationError::DivisionByZero { at: 3 })
    ));
    assert!(matches!(
        evaluate("0&&(1/0)"),
        Err(CalculationError::DivisionByZero { .. })
    ));
    assert!(matches!(
        evaluate("(1"),
        Err(CalculationError::MissingBracket { at: 2 })
    ));
    assert!(matches!(
        evaluate("0t10"),
        Err(CalculationError::Syntax { at: 2, .. })
    ));
    assert!(matches!(
        evaluate("123G"),
        Err(CalculationError::InvalidOperator { at: 3 })
    ));
    assert!(matches!(
        evaluate("@z"),
        Err(CalculationError::InvalidTag { at: 0 })
    ));
    assert!(matches!(
        evaluate("1=1"),
        Err(CalculationError::InvalidOperator { at: 1 })
    ));
    assert!(matches!(
        evaluate("\"ABCDEFGHI\""),
        Err(CalculationError::BigNumber { at: 0 })
    ));
    assert!(matches!(
        evaluate(&"-".repeat(MAX_EXPRESSION_DEPTH)),
        Err(CalculationError::NestingLimit)
    ));
    assert!(matches!(
        evaluate(&"0".repeat(MAX_EXPRESSION_BYTES + 1)),
        Err(CalculationError::LengthLimit)
    ));
}

#[test]
fn editor_evaluates_the_overlay_without_mutating_it() {
    let mut editor = Editor::new(document(&[0xFE, 0x80]), false);
    let offset = FileOffset::new(0);
    editor.document_mut().overwrite(offset, &[0xA5]).unwrap();
    assert_eq!(editor.calculate("@B").unwrap(), 0xA5);
    assert_eq!(editor.calculate_checked("@B+@o").unwrap(), 0xA5);
    assert_eq!(editor.cursor(), offset);
    assert!(editor.document().is_dirty());
    assert!(editor.document_mut().undo().unwrap());
    assert_eq!(editor.calculate("@B").unwrap(), 0xFE);
    assert!(!editor.document().is_dirty());
    assert_eq!(Command::OpenCalculator.id(), "tools.calculator");
}

#[test]
fn calculator_local_address_falls_back_to_file_offset_for_unmapped_image_bytes() {
    let mut bytes = crate::symbol_fixtures::pe64();
    let overlay = FileOffset::new(bytes.len() as u64);
    bytes.push(0xA5);
    let mut editor = Editor::new(document(&bytes), true);
    editor.dispatch(Command::Goto(overlay)).unwrap();
    assert_eq!(editor.calculate("@o").unwrap(), overlay.get());
    assert_eq!(editor.calculate_checked("@o").unwrap(), overlay.get());
    assert_eq!(editor.calculate("@O").unwrap(), overlay.get());
    assert_eq!(editor.calculate("@B:be").unwrap(), 0xA5);
    assert!(editor.executable().unwrap().file_to_va(overlay).is_err());
    assert_eq!(editor.cursor(), overlay);
    assert_eq!(editor.document().generation(), 0);
    assert!(!editor.document().is_dirty());
    assert_eq!(editor.document().source().as_slice(), bytes);
}

#[test]
fn calculator_raw_base_overflow_keeps_file_offsets_and_byte_reads_available() {
    let mut editor = Editor::new(document(&[0xAA, 0xBB, 0xCC]), true);
    editor.dispatch(Command::SetRaw(true)).unwrap();
    editor
        .dispatch(Command::SetRawBase(crate::formats::VirtualAddress::new(
            u64::MAX - 1,
        )))
        .unwrap();
    editor.dispatch(Command::Goto(FileOffset::new(1))).unwrap();
    assert_eq!(editor.calculate("@o").unwrap(), u64::MAX);
    assert_eq!(editor.calculate("@O").unwrap(), 1);
    editor.dispatch(Command::Goto(FileOffset::new(2))).unwrap();
    assert!(matches!(
        editor.calculate("@o"),
        Err(CalculationError::LocalOffsetOverflow { at: 0 })
    ));
    assert!(matches!(
        editor.calculate_checked("@o"),
        Err(CalculationError::LocalOffsetOverflow { at: 0 })
    ));
    assert_eq!(editor.calculate("@O").unwrap(), 2);
    assert_eq!(editor.calculate_checked("@B:be").unwrap(), 0xCC);
    assert_eq!(editor.cursor(), FileOffset::new(2));
    assert_eq!(editor.document().generation(), 0);
    assert!(!editor.document().is_dirty());
    assert_eq!(editor.document().source().as_slice(), [0xAA, 0xBB, 0xCC]);
}

#[test]
fn independent_expression_corpus_covers_each_precedence_level_and_associativity() {
    // These expected bit patterns are written independently of the parser's
    // operator table. They specify hiewbie's documented 8.63-compatible grammar,
    // not additional observations from the proprietary reference program.
    let mut data = document(&[]);
    for (expression, expected) in [
        ("1||0&&0", 1),
        ("0||2&&3", 1),
        ("1|2^3&1", 3),
        ("3^1&1", 2),
        ("1|1==2", 1),
        ("1<2==1", 1),
        ("1<2<1", 0),
        ("8>>1+1", 2),
        ("4+2<<1", 12),
        ("1<<1<<2", 8),
        ("10-3-2", 11),
        ("8/2*2", 8),
        ("2*3+1", 7),
        ("~1&3", 2),
        ("--1", 1),
        ("+(-1)", u64::MAX),
        ("!0+1", 2),
        ("!!2", 1),
        ("2!=3", 1),
        ("2!=2", 0),
        ("-1<=-1", 1),
        ("-1>=0", 0),
        ("-2>-1", 0),
        ("7/-3", u64::MAX - 1),
        ("-7/3", u64::MAX - 1),
        ("-7/-3", 2),
        ("7%-3", 1),
        ("-7%3", u64::MAX),
        ("8000000000000000<7FFFFFFFFFFFFFFF", 1),
        ("FFFFFFFFFFFFFFFF==(-1)", 1),
        ("FFFFffffFFFFffff", u64::MAX),
        ("18446744073709551615t", u64::MAX),
        ("1777777777777777777777o", u64::MAX),
        (
            "1111111111111111111111111111111111111111111111111111111111111111i",
            u64::MAX,
        ),
        ("1111I+17O+15T+FH+0Xf", 75),
        ("\"ABCDEFGH\"", 0x4142_4344_4546_4748),
        ("\"\"", 0),
        (" \t( 2 + 3 ) * 4\r\n", 20),
    ] {
        for checked in [false, true] {
            let actual = if checked {
                evaluate_checked(expression, &mut data, FileOffset::new(0), None)
            } else {
                evaluate(expression, &mut data, FileOffset::new(0), None)
            };
            assert_eq!(actual.unwrap(), expected, "{expression}, checked={checked}");
        }
    }
}

#[test]
fn grammar_errors_and_eager_logical_operands_are_explicit_in_both_modes() {
    let mut data = document(&[]);
    for checked in [false, true] {
        let mut run = |expression: &str| {
            if checked {
                evaluate_checked(expression, &mut data, FileOffset::new(0), None)
            } else {
                evaluate(expression, &mut data, FileOffset::new(0), None)
            }
        };
        for expression in ["0&&(1/0)", "1||(1%0)", "1/0", "1%0"] {
            assert!(
                matches!(
                    run(expression),
                    Err(CalculationError::DivisionByZero { .. })
                ),
                "{expression}, checked={checked}"
            );
        }
        for expression in ["2i", "8o", "At", "0x"] {
            assert!(
                matches!(
                    run(expression),
                    Err(CalculationError::InvalidNumber { at: 0 })
                ),
                "{expression}, checked={checked}"
            );
        }
        for expression in ["10000000000000000", "18446744073709551616t"] {
            assert!(
                matches!(run(expression), Err(CalculationError::BigNumber { at: 0 })),
                "{expression}, checked={checked}"
            );
        }
        for expression in ["", "1+", "1 2", "0xFFh", "1)", "(1+2", "\"open", "é"] {
            assert!(run(expression).is_err(), "{expression}, checked={checked}");
        }
    }
}

#[test]
fn all_cursor_value_widths_and_signs_have_explicit_byte_order() {
    // The leading byte makes every multi-byte read intentionally unaligned.
    let mut data = document(&[0, 0x81, 0xE3, 0x45, 0xE7, 0x89, 0xAB, 0xCD, 0xEF]);
    for (tag, little, big) in [
        ("@B", 0x81, 0x81),
        ("@b", 0xFFFF_FFFF_FFFF_FF81, 0xFFFF_FFFF_FFFF_FF81),
        ("@W", 0xE381, 0x81E3),
        ("@w", 0xFFFF_FFFF_FFFF_E381, 0xFFFF_FFFF_FFFF_81E3),
        ("@D", 0xE745_E381, 0x81E3_45E7),
        ("@d", 0xFFFF_FFFF_E745_E381, 0xFFFF_FFFF_81E3_45E7),
        ("@Q", 0xEFCD_AB89_E745_E381, 0x81E3_45E7_89AB_CDEF),
        ("@q", 0xEFCD_AB89_E745_E381, 0x81E3_45E7_89AB_CDEF),
    ] {
        for (suffix, expected) in [("", little), (":le", little), (":be", big), (":BE", big)] {
            let expression = format!("{tag}{suffix}");
            for checked in [false, true] {
                let result = if checked {
                    evaluate_checked(&expression, &mut data, FileOffset::new(1), None)
                } else {
                    evaluate(&expression, &mut data, FileOffset::new(1), None)
                };
                assert_eq!(result.unwrap(), expected, "{expression}, checked={checked}");
            }
        }
    }
    for (expression, expected) in [
        ("@W:be + @W:le", 0x1_6564),
        ("@W:bE - @W:Le", (-0x619E_i64) as u64),
        ("(@W:be)==81E3", 1),
        ("@W:be + @W", 0x1_6564),
        ("@o+@O+@B:be", 0x182),
    ] {
        assert_eq!(
            evaluate(expression, &mut data, FileOffset::new(1), Some(0x100)).unwrap(),
            expected,
            "{expression}"
        );
    }
}

#[test]
fn byte_order_and_sign_extension_cover_every_byte_pattern() {
    // Every possible byte is checked in positive and negative multi-byte
    // contexts. Expected values come from the independently specified bytes.
    for byte in 0_u16..=255 {
        let mut data = document(&[byte as u8, 0x80]);
        for (expression, expected) in [
            ("@B:be", u64::from(byte)),
            ("@b:le", ((byte as u8 as i8) as i64) as u64),
            ("@W:le", 0x8000 + u64::from(byte)),
            ("@w:le", (-32768_i64 + i64::from(byte)) as u64),
            ("@W:be", u64::from(byte) * 256 + 128),
            ("@w:be", (i64::from(byte as u8 as i8) * 256 + 128) as u64),
        ] {
            assert_eq!(
                evaluate(expression, &mut data, FileOffset::new(0), None).unwrap(),
                expected,
                "{expression}, byte={byte:02X}"
            );
        }
    }
}

struct ProbeSource {
    length: u64,
    max_read: usize,
    error: Option<io::ErrorKind>,
    calls: Vec<(FileOffset, usize)>,
}

impl ProbeSource {
    fn new(length: u64) -> Self {
        Self {
            length,
            max_read: 8,
            error: None,
            calls: Vec::new(),
        }
    }
}

impl ByteSource for ProbeSource {
    fn len(&self) -> u64 {
        self.length
    }

    fn read_at(&mut self, offset: FileOffset, out: &mut [u8]) -> io::Result<usize> {
        self.calls.push((offset, out.len()));
        if let Some(error) = self.error {
            return Err(io::Error::new(error, "generated source failure"));
        }
        let count = out.len().min(self.max_read);
        out[..count].fill(0xA5);
        Ok(count)
    }
}

#[test]
fn invalid_orders_and_widths_are_rejected_before_reading() {
    let mut data = Document::new(ProbeSource::new(8));
    for (expression, at) in [
        ("@W:", 2),
        ("@W:ne", 2),
        ("@W:little", 2),
        ("@W:be16", 2),
        ("@W:le_", 2),
        ("@W: be", 2),
        ("@W:é", 2),
        ("@W:be:le", 5),
        ("  @q:native", 4),
        ("@o:be", 2),
        ("@O:le", 2),
    ] {
        let error = evaluate(expression, &mut data, FileOffset::new(0), Some(0)).unwrap_err();
        assert!(
            matches!(error, CalculationError::InvalidByteOrder { at: actual } if actual == at),
            "{expression}: {error}"
        );
        assert_eq!(error.expression_offset(), Some(at));
        assert!(error.to_string().contains(":le/:be"));
    }
    for expression in ["@x:le", "@u16:be", "@128:be"] {
        assert!(matches!(
            evaluate(expression, &mut data, FileOffset::new(0), None),
            Err(CalculationError::InvalidTag { at: 0 })
        ));
    }
    assert!(data.source().calls.is_empty());
    assert_eq!(data.generation(), 0);
    assert!(!data.is_dirty());
}

#[test]
fn reads_at_eof_and_u64_boundaries_never_invent_or_wrap_bytes() {
    for (tag, width) in [("@B", 1), ("@W", 2), ("@D", 4), ("@Q", 8)] {
        for order in [":le", ":be"] {
            for length in 0..width {
                let mut data = Document::new(ProbeSource::new(length));
                assert!(matches!(
                    evaluate(&format!("{tag}{order}"), &mut data, FileOffset::new(0), None),
                    Err(CalculationError::ShortRead { width: actual, .. }) if actual as u64 == width
                ));
                assert!(!data.is_dirty());
                assert_eq!(data.generation(), 0);
            }
        }
    }

    let mut data = Document::new(ProbeSource::new(u64::MAX));
    assert_eq!(
        evaluate("@B:be", &mut data, FileOffset::new(u64::MAX - 1), None).unwrap(),
        0xA5
    );
    assert!(matches!(
        evaluate("@Q:be", &mut data, FileOffset::new(u64::MAX - 7), None),
        Err(CalculationError::ShortRead { width: 8, .. })
    ));
    assert!(matches!(
        evaluate("@W:le", &mut data, FileOffset::new(u64::MAX), None),
        Err(CalculationError::ShortRead { width: 2, .. })
    ));
    assert_eq!(
        data.source().calls,
        [
            (FileOffset::new(u64::MAX - 1), 1),
            (FileOffset::new(u64::MAX - 7), 7)
        ]
    );
    let mut small = document(&[1]);
    assert!(matches!(
        evaluate("@B:be", &mut small, FileOffset::new(u64::MAX), None),
        Err(CalculationError::Read(Error::OutOfBounds { .. }))
    ));
}

#[test]
fn source_failures_and_short_source_reads_keep_precise_errors() {
    let mut source = ProbeSource::new(8);
    source.error = Some(io::ErrorKind::PermissionDenied);
    let mut data = Document::new(source);
    assert!(matches!(
        evaluate("@D:be", &mut data, FileOffset::new(0), None),
        Err(CalculationError::Read(Error::Io(error))) if error.kind() == io::ErrorKind::PermissionDenied
    ));
    assert_eq!(data.generation(), 0);
    assert!(!data.is_dirty());

    let mut source = ProbeSource::new(8);
    source.max_read = 0;
    let mut data = Document::new(source);
    assert!(matches!(
        evaluate("@D:le", &mut data, FileOffset::new(0), None),
        Err(CalculationError::Read(Error::Io(error))) if error.kind() == io::ErrorKind::UnexpectedEof
    ));

    let mut source = ProbeSource::new(8);
    source.max_read = 1;
    let mut data = Document::new(source);
    assert_eq!(
        evaluate("@D:be", &mut data, FileOffset::new(0), None).unwrap(),
        0xA5A5_A5A5
    );
    assert_eq!(data.source().calls.len(), 4);
    assert_eq!(data.generation(), 0);
    assert!(!data.is_dirty());
}

#[test]
fn evaluation_preserves_transaction_history_cursor_and_search_continuation() {
    let original = [0xAA, 0x10, 0x20, 0xAA, 0x30, 0x40, 0xAA, 0x50, 0x60];
    let mut editor = Editor::new(document(&original), false);
    editor.dispatch(Command::BeginEdit).unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(1), &[1, 2])
        .unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(4), &[3])
        .unwrap();
    assert!(editor.document_mut().undo().unwrap());
    editor.start_search(SearchRequest::bytes(vec![0xAA]).unwrap());
    assert_eq!(
        editor.step_search(9).unwrap(),
        Some(SearchProgress::Found(FileOffset::new(0)))
    );
    editor.dispatch(Command::Move(1)).unwrap();
    let patches = editor.document().patches();
    let generation = editor.document().generation();
    let mode = editor.mode();

    for checked in [false, true] {
        for (expression, expected) in [("@W:be", 0x102), ("@W:le", 0x201), ("@o+@W:be", 0x103)] {
            let actual = if checked {
                editor.calculate_checked(expression)
            } else {
                editor.calculate(expression)
            };
            assert_eq!(actual.unwrap(), expected);
        }
    }
    for expression in ["@W:native", "1/0", "7FFFFFFFFFFFFFFF+1"] {
        assert!(editor.calculate_checked(expression).is_err());
    }
    assert_eq!(editor.cursor(), FileOffset::new(1));
    assert_eq!(editor.mode(), mode);
    assert!(editor.is_editing());
    assert!(editor.document().is_dirty());
    assert_eq!(editor.document().source().as_slice(), original);
    assert_eq!(editor.document().generation(), generation);
    assert_eq!(editor.document().patches(), patches);
    assert_eq!(editor.last_search_hit(), Some(FileOffset::new(0)));

    assert_eq!(editor.repeat_search(), SearchRepeat::Started);
    assert_eq!(
        editor.step_search(1).unwrap(),
        Some(SearchProgress::Pending {
            next: FileOffset::new(2)
        })
    );
    assert_eq!(editor.calculate("@W:be").unwrap(), 0x102);
    assert!(editor.calculate_checked("@O:be").is_err());
    assert!(editor.is_searching());
    assert_eq!(
        editor.step_search(7).unwrap(),
        Some(SearchProgress::Found(FileOffset::new(3)))
    );

    // Evaluation neither clears the pending redo nor introduces an undo entry.
    assert!(editor.document_mut().redo().unwrap());
    let mut logical = [0; 9];
    editor
        .document_mut()
        .read_at(FileOffset::new(0), &mut logical)
        .unwrap();
    assert_eq!(logical, [0xAA, 1, 2, 0xAA, 3, 0x40, 0xAA, 0x50, 0x60]);
    assert!(editor.document_mut().undo().unwrap());
    assert!(editor.document_mut().undo().unwrap());
    assert!(!editor.document_mut().undo().unwrap());
    assert!(!editor.document().is_dirty());
    editor
        .document_mut()
        .read_at(FileOffset::new(0), &mut logical)
        .unwrap();
    assert_eq!(logical, original);
}
