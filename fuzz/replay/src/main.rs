use std::{
    env,
    error::Error,
    fs,
    io::{self, Read},
    path::Path,
    process::ExitCode,
};

use hiewbie_fuzz_replay::{MAX_INPUT_BYTES, Target, replay, seeds};

fn read_bounded(reader: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(MAX_INPUT_BYTES + 1);
    reader
        .take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "artifact exceeds 4096-byte input limit",
        ));
    }
    Ok(bytes)
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => {
            for target in Target::ALL {
                println!(
                    "{}: {} deterministic cases passed",
                    target.name(),
                    replay(target)
                );
            }
        }
        [flag] if flag == "--list" => {
            for target in Target::ALL {
                println!("{}", target.name());
            }
        }
        [flag, directory] if flag == "--write-corpus" => {
            for seed in seeds() {
                let directory = Path::new(directory).join(seed.target.name());
                fs::create_dir_all(&directory)?;
                fs::write(directory.join(seed.name), seed.bytes)?;
            }
        }
        [flag, target, path] if flag == "--replay" => {
            let target = Target::parse(target.to_str().ok_or("non-UTF-8 target")?)
                .ok_or("unknown target")?;
            let metadata = fs::metadata(path)?;
            if !metadata.is_file() || metadata.len() > MAX_INPUT_BYTES as u64 {
                return Err("artifact must be a regular file of at most 4096 bytes".into());
            }
            let input = read_bounded(fs::File::open(path)?)?;
            target.check(&input);
            println!("{}: artifact passed", target.name());
        }
        _ => {
            return Err(
                "usage: replay [--list | --write-corpus DIRECTORY | --replay TARGET FILE]".into(),
            );
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("replay: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_INPUT_BYTES, read_bounded};

    #[test]
    fn oversized_artifact_is_rejected_after_bounded_read() {
        let mut input = std::io::Cursor::new(vec![0; MAX_INPUT_BYTES + 100]);
        assert!(read_bounded(&mut input).is_err());
        assert_eq!(input.position(), (MAX_INPUT_BYTES + 1) as u64);
        assert_eq!(
            read_bounded(std::io::Cursor::new(vec![7; MAX_INPUT_BYTES])).unwrap(),
            vec![7; MAX_INPUT_BYTES]
        );
    }
}
