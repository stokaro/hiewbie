//! Native command-line entry point. File bytes and commands live in the shared core.

mod terminal;

use std::env;
use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use hiewbie::file::FileSource;
use hiewbie_core::{ByteSource, Command, Document, Editor, FileOffset, parse_goto};

const HELP: &str = "hiewbie - a HIEW-inspired raw-byte terminal editor

Usage: hiewbie [OPTIONS] <FILE>

Options:
  -r, --read-only     Inspect without allowing an edit transaction
      --dump          Print the first 16 hex rows; no terminal is required
      --offset VALUE  Start at an offset (hexadecimal, or decimal with suffix t)
  -h, --help          Print this help
  -V, --version       Print the version
      --              Treat remaining arguments as filenames

Examples:
  hiewbie program.bin
  hiewbie --read-only program.bin
  hiewbie --dump --offset 0x100 program.bin
  hiewbie --dump --offset 256t program.bin

View: Enter cycles Hex/Text; F4 selects a mode; F3/Ctrl+E begins editing;
      F5/Ctrl+G goes to an offset; F7/Ctrl+F searches;
      Shift+F7, Ctrl+Enter, or Ctrl+R repeats the search.
Edit: F9/Ctrl+S saves; Esc discards; Tab switches hex/ASCII entry;
      F3 restores the original current byte; Backspace restores the previous;
      Ctrl+Z/Ctrl+Y undo/redo. A hex byte is applied after both digits.
      Backspace cancels an incomplete hex digit first.
F1 opens help. Esc exits from view mode. Bracketed paste is rejected.

This initial editor supports raw-byte overwrite transactions. Code mode,
executable parsing, assembly, blocks, and multiple files remain planned.
";

#[derive(Debug, Eq, PartialEq)]
struct Options {
    path: PathBuf,
    read_only: bool,
    dump: bool,
    offset: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
enum Invocation {
    Help,
    Version,
    Run(Options),
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    let mut args = args.into_iter();
    let mut path = None;
    let mut read_only = false;
    let mut dump = false;
    let mut offset = None;
    let mut positional_only = false;

    while let Some(arg) = args.next() {
        if !positional_only {
            match arg.to_str() {
                Some("--") => {
                    positional_only = true;
                    continue;
                }
                Some("--help" | "-h") => return Ok(Invocation::Help),
                Some("--version" | "-V") => return Ok(Invocation::Version),
                Some("--read-only" | "-r") => {
                    read_only = true;
                    continue;
                }
                Some("--dump") => {
                    dump = true;
                    continue;
                }
                Some("--offset") => {
                    let value = args.next().ok_or("--offset requires a value")?;
                    let value = value
                        .into_string()
                        .map_err(|_| "offset must be valid text")?;
                    if offset.replace(value).is_some() {
                        return Err("--offset may be supplied only once".into());
                    }
                    continue;
                }
                Some(value) if value.starts_with("--offset=") => {
                    if offset.replace(value[9..].to_owned()).is_some() {
                        return Err("--offset may be supplied only once".into());
                    }
                    continue;
                }
                _ => {}
            }
            if arg.as_encoded_bytes().first() == Some(&b'-') {
                return Err(format!(
                    "unknown option {}; use -- before a filename starting with -",
                    terminal::escaped_text(&arg.to_string_lossy())
                ));
            }
        }
        if path.replace(PathBuf::from(arg)).is_some() {
            return Err("only one file can be opened by this initial editor".into());
        }
    }

    Ok(Invocation::Run(Options {
        path: path.ok_or("a file is required; use --help for usage")?,
        read_only,
        dump,
        offset,
    }))
}

fn run(options: Options) -> Result<(), String> {
    let source = FileSource::open(&options.path).map_err(|error| {
        format!(
            "cannot open {}: {error}",
            terminal::escaped_text(&options.path.to_string_lossy())
        )
    })?;
    let mut editor = Editor::new(Document::new(source), options.read_only);
    if let Some(expression) = options.offset {
        let offset = parse_goto(&expression, editor.cursor(), editor.document().len())
            .map_err(|error| format!("invalid --offset: {error}"))?;
        editor
            .dispatch(Command::Goto(offset))
            .map_err(|error| error.to_string())?;
    }

    if options.dump {
        let offset = editor.cursor();
        return match write_dump(editor.document_mut(), offset, &mut io::stdout().lock()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            Err(error) => Err(format!("cannot print file: {error}")),
        };
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("interactive editing requires terminal input and output; use --dump for noninteractive inspection".into());
    }
    terminal::run(editor).map_err(|error| format!("terminal error: {error}"))
}

/// A deterministic, bounded first viewport, also useful through SSH or a pipe.
fn write_dump<S: ByteSource>(
    document: &mut Document<S>,
    offset: FileOffset,
    output: &mut impl Write,
) -> io::Result<()> {
    let mut buffer = [0u8; 16 * 16];
    let count = document
        .read_at(offset, &mut buffer)
        .map_err(io::Error::other)?;
    if count == 0 {
        return writeln!(output, "<empty file>");
    }
    for (row, bytes) in buffer[..count].chunks(16).enumerate() {
        // A returned byte necessarily has a representable offset in the source.
        let row_offset = offset.get().saturating_add((row * 16) as u64);
        write!(output, "{row_offset:016X}  ")?;
        for column in 0..16 {
            if column == 8 {
                write!(output, " ")?;
            }
            if let Some(byte) = bytes.get(column) {
                write!(output, "{byte:02X} ")?;
            } else {
                write!(output, "   ")?;
            }
        }
        write!(output, " |")?;
        for byte in bytes {
            write!(output, "{}", terminal::printable(*byte))?;
        }
        writeln!(output, "|")?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let result = match parse_args(env::args_os().skip(1)) {
        Ok(Invocation::Help) => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Ok(Invocation::Version) => {
            println!("hiewbie {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Ok(Invocation::Run(options)) => run(options),
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("hiewbie: {}", terminal::escaped_text(&error));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hiewbie_core::MemorySource;

    fn args(values: &[&str]) -> Result<Invocation, String> {
        parse_args(values.iter().map(OsString::from))
    }

    #[test]
    fn cli_help_version_and_invalid_invocations() {
        assert_eq!(args(&["--help"]).unwrap(), Invocation::Help);
        assert_eq!(args(&["--version"]).unwrap(), Invocation::Version);
        for invalid in [
            &[][..],
            &["a", "b"],
            &["--bad"],
            &["--offset"],
            &["--offset", "1", "--offset=2", "a"],
        ] {
            assert!(args(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn cli_preserves_filenames_and_offset_expression() {
        assert_eq!(
            args(&["--dump", "--offset=256t", "-r", "--", "-a b.bin"]).unwrap(),
            Invocation::Run(Options {
                path: PathBuf::from("-a b.bin"),
                read_only: true,
                dump: true,
                offset: Some("256t".into()),
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn cli_accepts_non_unicode_filename() {
        use std::os::unix::ffi::OsStringExt;
        let name = OsString::from_vec(vec![b'f', 0xff]);
        let Invocation::Run(options) = parse_args([name.clone()]).unwrap() else {
            panic!("expected a file invocation");
        };
        assert_eq!(options.path.into_os_string(), name);
    }

    #[test]
    fn dump_is_bounded_and_escapes_binary_data() {
        let mut document = Document::new(MemorySource::new((0..512).map(|n| n as u8).collect()));
        let mut output = Vec::new();
        write_dump(&mut document, FileOffset::new(0), &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.lines().count(), 16);
        assert!(output.starts_with("0000000000000000  00 01 02"));
        assert!(output.contains("00000000000000F0"));
        assert!(!output.contains("0000000000000100"));
        assert!(!output.contains('\x1b'));
    }

    #[test]
    fn dump_empty_document_and_unaligned_tail() {
        let mut empty = Document::new(MemorySource::new(Vec::new()));
        let mut output = Vec::new();
        write_dump(&mut empty, FileOffset::new(0), &mut output).unwrap();
        assert_eq!(output, b"<empty file>\n");
        let mut document = Document::new(MemorySource::new(b"ABC\0".to_vec()));
        output.clear();
        write_dump(&mut document, FileOffset::new(1), &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.starts_with("0000000000000001  42 43 00"));
        assert!(output.ends_with("|BC.|\n"));
    }
}
