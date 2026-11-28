use hiewbie_core::{
    Command, Document, Editor, FileOffset, MemorySource,
    formats::{Endian, VirtualAddress},
    instructions::{DecodeMode, MAX_INSTRUCTION_TEXT_BYTES, decode_one},
};

use crate::word;

pub(crate) fn check(data: &[u8]) {
    let modes = [
        DecodeMode::X86_16,
        DecodeMode::X86_32,
        DecodeMode::X86_64,
        DecodeMode::Arm,
        DecodeMode::Thumb,
        DecodeMode::Aarch64,
    ];
    let mode = modes[usize::from(data.first().copied().unwrap_or(0)) % modes.len()];
    let endian = if data.get(1).copied().unwrap_or(0) & 1 == 0 {
        Endian::Little
    } else {
        Endian::Big
    };
    let base = word(data, 2);
    let bytes = data.get(10..).unwrap_or_default();
    let mut position = 0;
    while position < bytes.len() {
        let address = VirtualAddress::new(base.wrapping_add(position as u64));
        let decoded = decode_one(&bytes[position..], address, mode, endian)
            .expect("nonempty decoder input must make progress");
        assert!((1..=mode.max_instruction_len()).contains(&decoded.length));
        assert!(decoded.length <= bytes.len() - position);
        assert!(decoded.text.len() <= MAX_INSTRUCTION_TEXT_BYTES);
        assert_eq!(decoded.bytes(), &bytes[position..position + decoded.length]);
        assert_eq!(decoded.address, address);
        position += decoded.length;
    }
    assert_eq!(position, bytes.len());
    assert!(decode_one(&[], VirtualAddress::new(base), mode, endian).is_none());

    // Exercise the editor adapter as well as the direct decoder/backends.
    let mut editor = Editor::new(Document::new(MemorySource::new(bytes.to_vec())), true);
    editor
        .dispatch(Command::SetRawBase(VirtualAddress::new(base)))
        .unwrap();
    editor.dispatch(Command::SetDecodeMode(Some(mode))).unwrap();
    editor.dispatch(Command::SetEndian(Some(endian))).unwrap();
    let _ = editor.code_rows(8);
    let mut raw = vec![0; bytes.len()];
    assert_eq!(
        editor
            .document_mut()
            .read_at(FileOffset::new(0), &mut raw)
            .unwrap(),
        bytes.len()
    );
    assert_eq!(raw, bytes);
    assert!(!editor.document().is_dirty());
    assert_eq!(editor.document().generation(), 0);
}
