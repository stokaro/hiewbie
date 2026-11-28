//! Shared, bounded fuzz properties. The stable replay and libFuzzer use these
//! exact functions; only the input producer and process isolation differ.

mod addresses;
mod corpus;
mod decoder;
mod edits;
mod formats;
mod search;
mod source;

pub use corpus::{Seed, seeds};

/// The limit is also passed to libFuzzer. Larger reproductions are rejected,
/// never silently truncated into a different test case.
pub const MAX_INPUT_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    FormatsSymbols,
    Decoder,
    Addresses,
    Search,
    EditCommands,
}

impl Target {
    pub const ALL: [Self; 5] = [
        Self::FormatsSymbols,
        Self::Decoder,
        Self::Addresses,
        Self::Search,
        Self::EditCommands,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::FormatsSymbols => "formats_symbols",
            Self::Decoder => "decoder",
            Self::Addresses => "addresses",
            Self::Search => "search",
            Self::EditCommands => "edit_commands",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|target| target.name() == name)
    }

    pub fn check(self, data: &[u8]) {
        assert!(data.len() <= MAX_INPUT_BYTES, "oversized fuzz input");
        match self {
            Self::FormatsSymbols => formats::check(data),
            Self::Decoder => decoder::check(data),
            Self::Addresses => addresses::check(data),
            Self::Search => search::check(data),
            Self::EditCommands => edits::check(data),
        }
    }
}

/// Run source-generated seeds and bounded, deterministic truncation/bit-flip
/// regressions. This is corpus replay, not coverage-guided fuzzing.
pub fn replay(target: Target) -> usize {
    let mut count = 0;
    for seed in seeds().into_iter().filter(|seed| seed.target == target) {
        target.check(&seed.bytes);
        count += 1;
        for length in [0, 1, 2, 3, 4, 7, 15, 31, seed.bytes.len() / 2] {
            if length < seed.bytes.len() {
                target.check(&seed.bytes[..length]);
                count += 1;
            }
        }
        for index in (0..seed.bytes.len()).step_by((seed.bytes.len() / 16).max(1)) {
            let mut changed = seed.bytes.clone();
            changed[index] ^= 0xff;
            target.check(&changed);
            count += 1;
        }
    }
    assert!(count > 0, "target has no replay corpus");
    count
}

/// Missing bytes read as zero, keeping short fuzz inputs useful and bounded.
pub(crate) fn word(data: &[u8], offset: usize) -> u64 {
    let mut value = [0; 8];
    for (destination, source) in value.iter_mut().zip(data.get(offset..).unwrap_or_default()) {
        *destination = *source;
    }
    u64::from_le_bytes(value)
}

#[cfg(test)]
mod tests {
    use super::{Target, replay};

    #[test]
    fn every_target_replays_the_portable_corpus() {
        for target in Target::ALL {
            assert!(replay(target) >= 2, "{}", target.name());
        }
    }
}
