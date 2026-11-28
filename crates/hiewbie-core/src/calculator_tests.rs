use super::*;
use crate::{Command, Editor, MemorySource};

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
    assert_eq!(editor.cursor(), offset);
    assert!(editor.document().is_dirty());
    assert!(editor.document_mut().undo().unwrap());
    assert_eq!(editor.calculate("@B").unwrap(), 0xFE);
    assert!(!editor.document().is_dirty());
    assert_eq!(Command::OpenCalculator.id(), "tools.calculator");
}
