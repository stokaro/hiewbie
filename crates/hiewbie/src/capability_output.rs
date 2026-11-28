//! The native CLI renders the same portable report used by headless callers.

use std::io::{self, Write};

use hiewbie_core::capabilities;

pub(super) fn write(json: bool, output: &mut impl Write) -> io::Result<()> {
    let report = capabilities::current();
    if json {
        serde_json::to_writer_pretty(&mut *output, &report).map_err(|error| {
            io::Error::new(error.io_error_kind().unwrap_or(io::ErrorKind::Other), error)
        })?;
        return writeln!(output);
    }
    writeln!(
        output,
        "hiewbie {} capabilities (schema {})",
        report.version, report.schema_version
    )?;
    writeln!(output, "{}\n", report.scope)?;
    writeln!(output, "{:<26} {:<11} BACKEND", "CAPABILITY", "STATUS")?;
    for entry in report.capabilities {
        write!(output, "{:<26} {:<11} ", entry.id, entry.status.to_string())?;
        match entry.backend {
            Some(backend) => writeln!(output, "{} {}", backend.name, backend.version)?,
            None => writeln!(output, "none")?,
        }
        if !entry.modes.is_empty() {
            writeln!(
                output,
                "  Modes: {}; byte order: {}",
                entry.modes.join(", "),
                entry.byte_orders.join(", ")
            )?;
        }
        for limitation in entry.limitations {
            writeln!(output, "  Limit: {limitation}")?;
        }
    }
    writeln!(
        output,
        "\nResearch probes, host targets, and browser availability are separate; see docs/capabilities.md."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_failures_are_reported_without_terminal_initialization() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        for json in [false, true] {
            assert_eq!(
                write(json, &mut Broken).unwrap_err().kind(),
                io::ErrorKind::BrokenPipe
            );
        }
    }
}
