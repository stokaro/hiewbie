//! Native command-line entry point. File bytes and commands live in the shared core.

mod symbol_output;
mod terminal;

use std::env;
use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use hiewbie::file::FileSource;
use hiewbie_core::formats::{Endian, VirtualAddress};
use hiewbie_core::instructions::DecodeMode;
use hiewbie_core::symbols::SymbolKind;
use hiewbie_core::{ByteSource, Command, Document, Editor, FileOffset, parse_virtual_address};

const HELP: &str = "hiewbie - a HIEW-inspired terminal hex editor and binary inspector

Usage: hiewbie [OPTIONS] [FILE ...]

Options:
  -r, --read-only      Inspect without allowing an edit transaction
      --dump          Print at most 16 hex rows without requiring a terminal
      --headers       Print bounded executable fields, load mappings, and slices
      --disassemble   Print at most 32 code rows without requiring a terminal
      --symbols       Print at most 64 parsed symbols, imports, and exports
      --imports       Print at most 64 parsed imports
      --exports       Print at most 64 parsed exports
      --offset VALUE  Start at a file offset, or an absolute VA prefixed by .
      --entry         Start at the parsed entry point; conflicts with --offset
      --arch MODE     auto, x86-16, x86-32, x86-64, arm, thumb, or aarch64
      --endian ORDER  Override instruction order: little or big
      --base VALUE    Raw virtual base (hex, or decimal with suffix t); implies raw
      --raw           Ignore executable mappings; raw code requires --arch
      --slice INDEX   Select a zero-based universal Mach-O slice (decimal)
  -h, --help           Print this help
  -V, --version        Print the version
      --              Treat remaining arguments as filenames

The dump/headers/disassemble/symbols/imports/exports modes are mutually exclusive.
Raw/base interpretation cannot be combined with --slice.

Examples:
  hiewbie program.bin
  hiewbie --read-only program.bin
  hiewbie --dump --offset 0x100 program.bin
  hiewbie --dump --offset 256t program.bin
  hiewbie --headers program.exe
  hiewbie --imports program.exe
  hiewbie --symbols --slice 1 universal-macho
  hiewbie --disassemble --entry program.exe
  hiewbie --disassemble --raw --arch x86-64 --base 0x400000 code.bin
  hiewbie --disassemble --slice 1 --entry universal-macho

Start without a file to open the file browser.
View: F9 opens the file browser; Enter cycles Text/Hex/Code; F4 selects a mode; F3/Ctrl+E begins editing;
      F5/Ctrl+G goes to an offset; F7/Ctrl+F searches;
      Shift+F7, Ctrl+Enter, or Ctrl+R repeats the search.
      + saves a screen, - restores it, and Alt+1 through Alt+8 select bookmarks.
      Alt+- removes the active bookmark. Alt+0 clears all view bookmarks.
      Ctrl+Backspace opens History; Tab cycles visited files.
      Ctrl+F9 selects command-line files; Ctrl+F11/F12 moves between them.
Code: F8/Ctrl+O shows headers; Shift+F1/Ctrl+A opens the analysis profile.
      Ctrl+F1/Ctrl+B cycles x86 bitness; / resynchronizes; Backspace returns.
      The profile selects architecture, instruction byte order, and raw base.
      Headers: S selects a slice; F7 imports; F9 exports; N all parsed symbols.
      Symbol tables: Enter follows target bytes; R follows the metadata record.
      Branch numbers follow, never execute.
Edit: F9/Ctrl+S saves; Esc discards; Tab switches hex/ASCII entry;
      F3 restores the original current byte; Backspace restores the previous;
      Ctrl+Z/Ctrl+Y undo/redo. A hex byte is applied after both digits.
      Backspace cancels an incomplete hex digit first.
F1 opens help. Esc exits from view mode. Bracketed paste is rejected.

PE/ELF/Mach-O inspection covers headers, load mappings, and selected symbol tables.
Symbols are loaded on request; coverage notes identify unparsed loader metadata.
Relocations and textual assembly remain planned.
Big-endian instruction decoding is explicitly unsupported.
";

const MAX_HEADER_ITEMS: usize = 64;
const MAX_DISASSEMBLY_ROWS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputMode {
    Interactive,
    Dump,
    Headers,
    Disassemble,
    Symbols,
    Imports,
    Exports,
}

#[derive(Debug, Eq, PartialEq)]
struct Options {
    path: PathBuf,
    additional_paths: Vec<PathBuf>,
    read_only: bool,
    output: OutputMode,
    offset: Option<String>,
    entry: bool,
    architecture: Option<DecodeMode>,
    endian: Option<Endian>,
    base: Option<VirtualAddress>,
    raw: bool,
    slice: Option<usize>,
}

#[derive(Debug, Eq, PartialEq)]
enum Invocation {
    Help,
    Version,
    Browse { read_only: bool },
    Run(Options),
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    let mut args = args.into_iter();
    let mut paths = Vec::new();
    let mut read_only = false;
    let mut output = None;
    let mut offset = None;
    let mut entry = false;
    let mut architecture = None;
    let mut endian = None;
    let mut base = None;
    let mut raw = false;
    let mut slice = None;
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
                Some(
                    "--dump" | "--headers" | "--disassemble" | "--symbols" | "--imports"
                    | "--exports",
                ) => {
                    let mode = match arg.to_str() {
                        Some("--dump") => OutputMode::Dump,
                        Some("--headers") => OutputMode::Headers,
                        Some("--symbols") => OutputMode::Symbols,
                        Some("--imports") => OutputMode::Imports,
                        Some("--exports") => OutputMode::Exports,
                        _ => OutputMode::Disassemble,
                    };
                    if output.replace(mode).is_some() {
                        return Err(
                            "--dump, --headers, --disassemble, --symbols, --imports, and --exports are mutually exclusive".into(),
                        );
                    }
                    continue;
                }
                Some("--entry") => {
                    entry = true;
                    continue;
                }
                Some("--raw") => {
                    raw = true;
                    continue;
                }
                _ => {}
            }
            if let Some(argument) = arg.to_str() {
                let (flag, inline) = argument
                    .split_once('=')
                    .map_or((argument, None), |(flag, value)| (flag, Some(value)));
                if matches!(
                    flag,
                    "--offset" | "--arch" | "--endian" | "--base" | "--slice"
                ) {
                    let value = match inline {
                        Some(value) => value.to_owned(),
                        None => args
                            .next()
                            .ok_or_else(|| format!("{flag} requires a value"))?
                            .into_string()
                            .map_err(|_| format!("{flag} must be valid text"))?,
                    };
                    match flag {
                        "--offset" => set_once(&mut offset, value, flag)?,
                        "--arch" => {
                            let mode = if value == "auto" {
                                None
                            } else {
                                Some(
                                    value
                                        .parse::<DecodeMode>()
                                        .map_err(|error| format!("invalid --arch: {error}"))?,
                                )
                            };
                            set_once(&mut architecture, mode, flag)?;
                        }
                        "--endian" => {
                            let order = match value.as_str() {
                                "little" => Endian::Little,
                                "big" => Endian::Big,
                                _ => return Err("--endian must be little or big".into()),
                            };
                            set_once(&mut endian, order, flag)?;
                        }
                        "--base" => set_once(
                            &mut base,
                            parse_virtual_address(&value)
                                .map_err(|error| format!("invalid --base: {error}"))?,
                            flag,
                        )?,
                        "--slice" => {
                            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit())
                            {
                                return Err("--slice requires a non-negative decimal index".into());
                            }
                            set_once(
                                &mut slice,
                                value
                                    .parse::<usize>()
                                    .map_err(|_| "--slice index is too large")?,
                                flag,
                            )?;
                        }
                        _ => unreachable!(),
                    }
                    continue;
                }
            }
            if arg.as_encoded_bytes().first() == Some(&b'-') {
                return Err(format!(
                    "unknown option {}; use -- before a filename starting with -",
                    terminal::escaped_text(&arg.to_string_lossy())
                ));
            }
        }
        paths.push(PathBuf::from(arg));
    }

    if entry && offset.is_some() {
        return Err("--entry and --offset are mutually exclusive".into());
    }
    if slice.is_some() && (raw || base.is_some()) {
        return Err("--slice cannot be combined with --raw or --base".into());
    }
    let mut paths = paths.into_iter();
    let Some(path) = paths.next() else {
        if output.is_some()
            || entry
            || offset.is_some()
            || architecture.is_some()
            || endian.is_some()
            || base.is_some()
            || raw
            || slice.is_some()
        {
            return Err("a file is required for inspection and startup position options".into());
        }
        return Ok(Invocation::Browse { read_only });
    };
    if output.is_some() && paths.len() != 0 {
        return Err("noninteractive inspection accepts one file".into());
    }
    Ok(Invocation::Run(Options {
        path,
        additional_paths: paths.collect(),
        read_only,
        output: output.unwrap_or(OutputMode::Interactive),
        offset,
        entry,
        architecture: architecture.flatten(),
        endian,
        base,
        raw,
        slice,
    }))
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), String> {
    if slot.replace(value).is_some() {
        return Err(format!("{flag} may be supplied only once"));
    }
    Ok(())
}

fn run(options: Options) -> Result<(), String> {
    let source = FileSource::open(&options.path).map_err(|error| {
        format!(
            "cannot open {}: {error}",
            terminal::escaped_text(&options.path.to_string_lossy())
        )
    })?;
    let mut editor = Editor::new(Document::new(source), options.read_only);
    if options.raw {
        editor
            .dispatch(Command::SetRaw(true))
            .map_err(|error| error.to_string())?;
    }
    if let Some(base) = options.base {
        editor
            .dispatch(Command::SetRawBase(base))
            .map_err(|error| error.to_string())?;
    }
    if let Some(mode) = options.architecture {
        editor
            .dispatch(Command::SetDecodeMode(Some(mode)))
            .map_err(|error| error.to_string())?;
    }
    if let Some(endian) = options.endian {
        editor
            .dispatch(Command::SetEndian(Some(endian)))
            .map_err(|error| error.to_string())?;
    }
    if let Some(slice) = options.slice {
        editor
            .dispatch(Command::SelectSlice(slice))
            .map_err(|error| format!("invalid --slice: {error}"))?;
    }
    if options.entry {
        editor
            .dispatch(Command::GotoEntry)
            .map_err(|error| format!("invalid --entry: {error}"))?;
    }
    if let Some(expression) = options.offset {
        editor
            .goto_expression(&expression)
            .map_err(|error| format!("invalid --offset: {error}"))?;
    }

    if options.output != OutputMode::Interactive {
        let offset = editor.cursor();
        let mut output = io::stdout().lock();
        let result = match options.output {
            OutputMode::Dump => write_dump(editor.document_mut(), offset, &mut output),
            OutputMode::Headers => write_headers(&mut editor, &mut output),
            OutputMode::Disassemble => write_disassembly(&mut editor, &mut output),
            OutputMode::Symbols => symbol_output::write(&mut editor, None, &mut output),
            OutputMode::Imports => {
                symbol_output::write(&mut editor, Some(SymbolKind::Import), &mut output)
            }
            OutputMode::Exports => {
                symbol_output::write(&mut editor, Some(SymbolKind::Export), &mut output)
            }
            OutputMode::Interactive => unreachable!(),
        };
        return match result {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            Err(error) => Err(format!("cannot print inspection: {error}")),
        };
    }
    run_terminal(
        Some((editor, &options.path)),
        options.read_only,
        options.additional_paths,
    )
}

fn run_terminal(
    start: Option<(Editor<FileSource>, &std::path::Path)>,
    read_only: bool,
    additional_paths: Vec<PathBuf>,
) -> Result<(), String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("interactive editing requires terminal input and output; use --dump, --headers, --disassemble, --symbols, --imports, or --exports for noninteractive inspection".into());
    }
    terminal::run(start, read_only, additional_paths)
        .map_err(|error| format!("terminal error: {error}"))
}

fn write_headers<S: ByteSource>(editor: &mut Editor<S>, output: &mut impl Write) -> io::Result<()> {
    editor.refresh_analysis();
    let image = editor.executable().ok_or_else(|| {
        io::Error::other(format!(
            "{}; use --dump for raw bytes",
            editor
                .analysis_problem()
                .unwrap_or("no executable metadata is available")
        ))
    })?;
    writeln!(output, "Format: {}", image.format)?;
    writeln!(
        output,
        "Machine: {}; {}-bit; {}-endian",
        image.machine, image.bitness, image.endian
    )?;
    writeln!(output, "Generation: {}", image.generation)?;
    writeln!(
        output,
        "Image: FO {:016X}; {} file bytes",
        image.slice.offset.get(),
        image.slice.len
    )?;
    if let Some(base) = image.image_base {
        writeln!(output, "Image base: VA {:016X}", base.get())?;
    }
    if let Some(entry) = image.entry {
        writeln!(output, "Entry: VA {:016X}", entry.get())?;
    } else {
        writeln!(output, "Entry: unavailable")?;
    }
    if let Some(index) = image.selected_slice {
        writeln!(
            output,
            "Selected slice: {index} of {} (zero-based)",
            image.slices.len()
        )?;
    }
    writeln!(output, "Regions ({}):", image.regions.len())?;
    for (index, region) in image.regions.iter().take(MAX_HEADER_ITEMS).enumerate() {
        writeln!(
            output,
            "  [{index}] {} FO {:016X}+{:X} VA {:016X}+{:X} {}{}{}; header FO {:016X}",
            terminal::escaped_text(&region.name),
            region.file.offset.get(),
            region.file.len,
            region.address.get(),
            region.memory_size,
            if region.readable { 'R' } else { '-' },
            if region.writable { 'W' } else { '-' },
            if region.executable { 'X' } else { '-' },
            region.header.offset.get()
        )?;
    }
    omitted(output, "regions", image.regions.len())?;
    writeln!(output, "Fields ({}):", image.fields.len())?;
    for field in image.fields.iter().take(MAX_HEADER_ITEMS) {
        writeln!(
            output,
            "  {} = 0x{:X}; FO {:016X}+{}",
            terminal::escaped_text(&field.name),
            field.value,
            field.range.offset.get(),
            field.range.len
        )?;
    }
    omitted(output, "fields", image.fields.len())?;
    if !image.slices.is_empty() {
        writeln!(output, "Slices ({}):", image.slices.len())?;
        for slice in image.slices.iter().take(MAX_HEADER_ITEMS) {
            writeln!(
                output,
                "  [{}] {} FO {:016X}+{:X}; CPU 0x{:X}/0x{:X}",
                slice.index,
                slice.machine,
                slice.file.offset.get(),
                slice.file.len,
                slice.cpu_type,
                slice.cpu_subtype
            )?;
        }
        omitted(output, "slices", image.slices.len())?;
    }
    writeln!(
        output,
        "Scope: image headers and load mappings; use --symbols, --imports, or --exports for on-demand tables and their coverage notes. Relocations are not interpreted."
    )
}

fn omitted(output: &mut impl Write, category: &str, count: usize) -> io::Result<()> {
    if count > MAX_HEADER_ITEMS {
        writeln!(
            output,
            "  ... {} additional {category} omitted",
            count - MAX_HEADER_ITEMS
        )?;
    }
    Ok(())
}

fn write_disassembly<S: ByteSource>(
    editor: &mut Editor<S>,
    output: &mut impl Write,
) -> io::Result<()> {
    let rows = editor
        .code_rows(MAX_DISASSEMBLY_ROWS)
        .map_err(io::Error::other)?;
    let context = editor.decode_context();
    let mode = context
        .mode
        .ok_or_else(|| io::Error::other("raw code requires an explicit --arch"))?;
    writeln!(
        output,
        "Profile: {mode}; {}-endian; backend {}; {}{}",
        context.endian,
        mode.backend(),
        if context.raw { "raw" } else { "parsed" },
        if context.overridden {
            "; explicit override"
        } else {
            ""
        }
    )?;
    if context.raw {
        writeln!(output, "Raw base: VA {:016X}", context.base.get())?;
    }
    if let Some(image) = editor.executable() {
        writeln!(
            output,
            "Image: {}; declared {}; FO {:016X}+{:X}",
            image.format,
            image.machine,
            image.slice.offset.get(),
            image.slice.len
        )?;
        if let Some(index) = image.selected_slice {
            writeln!(
                output,
                "Selected slice: {index} of {} (zero-based)",
                image.slices.len()
            )?;
        }
    }
    if let Some(problem) = editor.analysis_problem() {
        writeln!(
            output,
            "Format fallback: {}",
            terminal::escaped_text(problem)
        )?;
    }
    if rows.is_empty() {
        return writeln!(output, "<empty file>");
    }
    for row in rows {
        write!(
            output,
            "FO {:016X} {} {:016X}  ",
            row.offset.get(),
            if row.mapped { "VA" } else { "FO" },
            row.instruction.address.get()
        )?;
        for byte in row.instruction.bytes() {
            write!(output, "{byte:02X}")?;
        }
        write!(
            output,
            "  {}",
            terminal::escaped_text(&row.instruction.text)
        )?;
        if let Some(target) = row.instruction.direct_branch {
            write!(
                output,
                "  -> VA {:016X} ({})",
                target.address.get(),
                target.mode
            )?;
        }
        writeln!(output)?;
    }
    Ok(())
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
        Ok(Invocation::Browse { read_only }) => run_terminal(None, read_only, Vec::new()),
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
        assert_eq!(args(&[]).unwrap(), Invocation::Browse { read_only: false });
        assert_eq!(
            args(&["-r"]).unwrap(),
            Invocation::Browse { read_only: true }
        );
        for invalid in [
            &["--dump"][..],
            &["--entry"],
            &["--offset", "12"],
            &["--dump", "a", "b"],
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
                additional_paths: Vec::new(),
                read_only: true,
                output: OutputMode::Dump,
                offset: Some("256t".into()),
                entry: false,
                architecture: None,
                endian: None,
                base: None,
                raw: false,
                slice: None,
            })
        );
    }

    #[test]
    fn cli_keeps_multiple_native_paths_in_argument_order() {
        let Invocation::Run(options) =
            args(&["first.bin", "--", "-second file.bin", "first.bin"]).unwrap()
        else {
            panic!("expected interactive invocation");
        };
        assert_eq!(options.path, PathBuf::from("first.bin"));
        assert_eq!(
            options.additional_paths,
            [
                PathBuf::from("-second file.bin"),
                PathBuf::from("first.bin")
            ]
        );
    }

    #[test]
    fn cli_rejects_conflicting_or_repeated_analysis_options() {
        for invalid in [
            &["--dump", "--headers", "file"][..],
            &["--headers", "--disassemble", "file"],
            &["--entry", "--offset=10", "file"],
            &["--raw", "--slice=0", "file"],
            &["--base=0", "--slice=0", "file"],
            &["--arch=auto", "--arch=x86-64", "file"],
            &["--arch=not-an-isa", "file"],
            &["--endian=middle", "file"],
            &["--slice=-1", "file"],
            &["--base=+100", "file"],
        ] {
            assert!(args(invalid).is_err(), "accepted {invalid:?}");
        }
        for first in [
            "--dump",
            "--headers",
            "--disassemble",
            "--symbols",
            "--imports",
            "--exports",
        ] {
            for second in [
                "--dump",
                "--headers",
                "--disassemble",
                "--symbols",
                "--imports",
                "--exports",
            ] {
                assert!(
                    args(&[first, second, "file"]).is_err(),
                    "accepted {first} with {second}"
                );
            }
        }
    }

    #[test]
    fn cli_parses_explicit_raw_profile_without_assuming_the_host() {
        let Invocation::Run(options) = args(&[
            "--disassemble",
            "--arch",
            "arm64",
            "--endian",
            "little",
            "--base",
            "4096t",
            "--raw",
            "file",
        ])
        .unwrap() else {
            panic!("expected run");
        };
        assert_eq!(options.output, OutputMode::Disassemble);
        assert_eq!(options.architecture, Some(DecodeMode::Aarch64));
        assert_eq!(options.endian, Some(Endian::Little));
        assert_eq!(options.base, Some(VirtualAddress::new(4096)));
        assert!(options.raw);
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
