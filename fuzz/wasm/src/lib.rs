//! Minimal no-pointer export boundary for the same portable invariant handlers.
//! The workspace replay/core crates continue to forbid unsafe code.

/// Export a uniquely named, integer-only entry point. The unsafe attribute
/// controls the linker symbol; the function contains no unsafe operations.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn hiewbie_replay_fuzz_target(index: u32) -> u32 {
    let Some(target) = hiewbie_fuzz_replay::Target::ALL.get(index as usize) else {
        return 0;
    };
    hiewbie_fuzz_replay::replay(*target) as u32
}
