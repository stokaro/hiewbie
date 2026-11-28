#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    hiewbie_fuzz_replay::Target::Search.check(input);
});
