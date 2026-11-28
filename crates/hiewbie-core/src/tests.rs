use std::{cell::Cell, io, mem::size_of, rc::Rc};

use crate::*;

fn offset(value: u64) -> FileOffset {
    FileOffset::new(value)
}

fn memory(bytes: &[u8]) -> Document<MemorySource> {
    Document::new(MemorySource::new(bytes.to_vec()))
}

fn logical<S: ByteSource>(document: &mut Document<S>) -> Vec<u8> {
    let mut bytes = vec![0; document.len() as usize];
    let mut start = 0;
    while start < bytes.len() {
        let end = (start + MAX_READ_BYTES).min(bytes.len());
        let read = document
            .read_at(offset(start as u64), &mut bytes[start..end])
            .unwrap();
        assert_eq!(read, end - start);
        start = end;
    }
    bytes
}

#[test]
fn bookmarks_fill_the_first_free_slot_without_replacing_or_renumbering() {
    use BookmarkCommand::*;
    use BookmarkMark::*;

    let mut editor = Editor::new(memory(&[0xA5; 64]), false);
    let viewport = BookmarkViewport {
        top: offset(1),
        column: 0,
    };
    for index in 0..BOOKMARK_SLOTS {
        editor
            .dispatch(Command::Goto(offset(index as u64 + 1)))
            .unwrap();
        editor.dispatch(Command::Bookmark(Store(viewport))).unwrap();
    }
    editor.dispatch(Command::Goto(offset(61))).unwrap();
    editor.dispatch(Command::Bookmark(Store(viewport))).unwrap();
    assert_eq!(editor.cursor(), offset(61));
    assert_eq!(editor.bookmark_marks()[7], Current);
    assert_eq!(
        editor.dispatch(Command::Bookmark(Recall)).unwrap(),
        Effect::RestoreBookmark(viewport)
    );
    assert_eq!(editor.cursor(), offset(8));
    editor.dispatch(Command::Bookmark(Select(2))).unwrap();
    assert_eq!(editor.cursor(), offset(2));

    editor.dispatch(Command::Bookmark(Delete)).unwrap();
    editor.dispatch(Command::Goto(offset(32))).unwrap();
    for action in [Recall, Select(2), Select(0), Select(9)] {
        assert_eq!(
            editor.dispatch(Command::Bookmark(action)).unwrap(),
            Effect::None
        );
        assert_eq!(editor.cursor(), offset(32));
    }
    assert_eq!(editor.bookmark_marks()[1], Empty);
    assert_eq!(editor.bookmark_marks()[2], Stored);

    editor.dispatch(Command::Bookmark(Store(viewport))).unwrap();
    assert_eq!(editor.bookmark_marks()[1], Current);
    editor.dispatch(Command::Bookmark(Select(3))).unwrap();
    assert_eq!(editor.cursor(), offset(3));
    editor.dispatch(Command::Bookmark(Select(2))).unwrap();
    assert_eq!(editor.cursor(), offset(32));
    assert_eq!(logical(editor.document_mut()), [0xA5; 64]);
    assert!(!editor.document().is_dirty());
}

#[test]
fn bookmark_recall_is_per_view_clear_is_global_and_edits_are_guarded() {
    use BookmarkCommand::*;
    use BookmarkMark::*;

    let viewport = BookmarkViewport::default();
    let mut editor = Editor::new(memory(b"0123456789"), false);
    let views = [(ViewMode::Hex, 2), (ViewMode::Text, 4), (ViewMode::Code, 6)];
    for (mode, position) in views {
        editor.dispatch(Command::SetMode(mode)).unwrap();
        assert_eq!(editor.bookmark_marks(), [Empty; BOOKMARK_SLOTS]);
        editor.dispatch(Command::Goto(offset(position))).unwrap();
        editor.dispatch(Command::Bookmark(Store(viewport))).unwrap();
    }
    for (mode, position) in views {
        editor.dispatch(Command::SetMode(mode)).unwrap();
        editor.dispatch(Command::Goto(offset(0))).unwrap();
        editor.dispatch(Command::Bookmark(Select(1))).unwrap();
        assert_eq!(editor.cursor(), offset(position));
        assert_eq!(editor.mode(), mode);
    }

    editor.dispatch(Command::BeginEdit).unwrap();
    editor.dispatch(Command::WriteByte(b'Z')).unwrap();
    for action in [Store(viewport), Recall, Select(1), Delete, Clear] {
        let command = Command::Bookmark(action);
        assert!(matches!(
            editor.dispatch(command),
            Err(Error::EditingInProgress)
        ));
    }
    assert!(editor.is_editing());
    assert!(editor.document().is_dirty());
    assert_eq!(logical(editor.document_mut()), b"012345Z789");
    editor.dispatch(Command::DiscardEdit).unwrap();
    editor.dispatch(Command::Bookmark(Clear)).unwrap();
    for (mode, _) in views {
        editor.dispatch(Command::SetMode(mode)).unwrap();
        assert_eq!(editor.bookmark_marks(), [Empty; BOOKMARK_SLOTS]);
    }
    assert_eq!(logical(editor.document_mut()), b"0123456789");
}

#[test]
fn code_bookmarks_restore_decoder_mode_without_changing_the_raw_base() {
    use instructions::DecodeMode;

    let mut editor = Editor::new(memory(&[0x90; 64]), true);
    editor.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
    editor
        .dispatch(Command::SetDecodeMode(Some(DecodeMode::X86_32)))
        .unwrap();
    editor.dispatch(Command::Goto(offset(7))).unwrap();
    editor
        .dispatch(Command::Bookmark(BookmarkCommand::Store(
            BookmarkViewport::default(),
        )))
        .unwrap();
    editor
        .dispatch(Command::SetRawBase(formats::VirtualAddress::new(0x8000)))
        .unwrap();
    editor
        .dispatch(Command::SetDecodeMode(Some(DecodeMode::X86_64)))
        .unwrap();
    editor.dispatch(Command::Goto(offset(16))).unwrap();
    editor
        .dispatch(Command::Bookmark(BookmarkCommand::Recall))
        .unwrap();

    assert_eq!(editor.cursor(), offset(7));
    assert_eq!(editor.decode_mode_override(), Some(DecodeMode::X86_32));
    assert_eq!(editor.decode_context().base.get(), 0x8000);
    assert!(editor.is_read_only());
    assert_eq!(logical(editor.document_mut()), [0x90; 64]);
}

#[test]
fn overlay_reads_patches_and_original_bytes_are_exact() {
    let mut document = memory(b"0123456789");
    document.overwrite(offset(2), b"AB").unwrap();
    document.overwrite(offset(4), b"C").unwrap();
    document.overwrite(offset(8), b"Z").unwrap();
    assert_eq!(logical(&mut document), b"01ABC567Z9");
    assert_eq!(document.source().as_slice(), b"0123456789");
    assert!(document.is_dirty());
    assert_eq!(
        document.patches(),
        vec![
            Patch {
                offset: offset(2),
                bytes: b"ABC".to_vec()
            },
            Patch {
                offset: offset(8),
                bytes: b"Z".to_vec()
            },
        ]
    );
}

#[test]
fn overlapping_overwrites_undo_redo_and_discard_restore_exact_states() {
    let mut document = memory(b"abcdef");
    document.overwrite(offset(1), b"XYZ").unwrap();
    document.overwrite(offset(2), b"12").unwrap();
    assert_eq!(logical(&mut document), b"aX12ef");
    assert!(document.undo().unwrap());
    assert_eq!(logical(&mut document), b"aXYZef");
    assert!(document.undo().unwrap());
    assert_eq!(logical(&mut document), b"abcdef");
    assert!(!document.is_dirty());
    assert!(!document.undo().unwrap());
    assert!(document.redo().unwrap());
    assert_eq!(logical(&mut document), b"aXYZef");
    assert!(document.redo().unwrap());
    assert_eq!(logical(&mut document), b"aX12ef");
    assert!(!document.redo().unwrap());
    document.discard();
    assert_eq!(logical(&mut document), b"abcdef");
    assert!(!document.is_dirty());
    assert!(!document.undo().unwrap());
    assert!(!document.redo().unwrap());
}

#[test]
fn restoring_source_values_removes_dirty_bytes_and_remains_undoable() {
    let mut document = memory(b"abc");
    document.overwrite(offset(1), b"X").unwrap();
    document.overwrite(offset(1), b"b").unwrap();
    assert!(!document.is_dirty());
    assert!(document.patches().is_empty());
    assert_eq!(logical(&mut document), b"abc");
    document.undo().unwrap();
    assert_eq!(logical(&mut document), b"aXc");
    document.redo().unwrap();
    assert_eq!(logical(&mut document), b"abc");
}

#[test]
fn byte_restore_uses_the_source_value_and_has_independent_undo_history() {
    let mut editor = Editor::new(memory(&[0x11, 0x22, 0x33]), false);
    editor.dispatch(Command::BeginEdit).unwrap();
    editor.dispatch(Command::WriteByte(0xAA)).unwrap();
    editor.dispatch(Command::Goto(offset(0))).unwrap();
    editor.dispatch(Command::WriteByte(0xBB)).unwrap();
    editor.dispatch(Command::WriteByte(0xCC)).unwrap();
    editor.dispatch(Command::Goto(offset(0))).unwrap();
    editor.dispatch(Command::RestoreByte).unwrap();
    assert_eq!(logical(editor.document_mut()), [0x11, 0xCC, 0x33]);
    assert_eq!(editor.cursor(), offset(0));
    assert!(editor.is_editing());
    assert!(editor.document().is_dirty());
    editor.dispatch(Command::Undo).unwrap();
    assert_eq!(logical(editor.document_mut()), [0xBB, 0xCC, 0x33]);
    editor.dispatch(Command::Redo).unwrap();
    assert_eq!(logical(editor.document_mut()), [0x11, 0xCC, 0x33]);

    editor.dispatch(Command::Goto(offset(2))).unwrap();
    editor.dispatch(Command::RestorePreviousByte).unwrap();
    assert_eq!(logical(editor.document_mut()), [0x11, 0x22, 0x33]);
    assert_eq!(editor.cursor(), offset(1));
    assert!(!editor.document().is_dirty());
    editor.dispatch(Command::Undo).unwrap();
    assert_eq!(logical(editor.document_mut()), [0x11, 0xCC, 0x33]);
    editor.dispatch(Command::Redo).unwrap();
    assert_eq!(logical(editor.document_mut()), [0x11, 0x22, 0x33]);

    editor.dispatch(Command::Goto(offset(0))).unwrap();
    let generation = editor.document().generation();
    editor.dispatch(Command::RestorePreviousByte).unwrap();
    assert_eq!(editor.cursor(), offset(0));
    assert_eq!(editor.document().generation(), generation);
    editor.dispatch(Command::Undo).unwrap();
    assert_eq!(logical(editor.document_mut()), [0x11, 0xCC, 0x33]);
}

#[test]
fn restore_no_op_and_invalid_ranges_preserve_redo_and_generation() {
    let mut document = memory(b"abc");
    document.overwrite(offset(1), b"X").unwrap();
    document.undo().unwrap();
    let generation = document.generation();
    document.restore_byte(offset(1)).unwrap();
    for position in [3, u64::MAX] {
        assert!(matches!(
            document.restore_byte(offset(position)),
            Err(Error::OutOfBounds { .. })
        ));
    }
    assert_eq!(document.generation(), generation);
    assert_eq!(logical(&mut document), b"abc");
    assert!(!document.is_dirty());
    assert!(document.redo().unwrap());
    assert_eq!(logical(&mut document), b"aXc");

    document.rebase(MemorySource::new(b"aXc".to_vec()));
    document.overwrite(offset(1), b"Y").unwrap();
    document.restore_byte(offset(1)).unwrap();
    assert_eq!(logical(&mut document), b"aXc");
    assert!(!document.is_dirty());

    let mut empty = Editor::new(memory(b""), false);
    empty.dispatch(Command::BeginEdit).unwrap();
    assert!(matches!(
        empty.dispatch(Command::RestoreByte),
        Err(Error::OutOfBounds { .. })
    ));
    empty.dispatch(Command::RestorePreviousByte).unwrap();
    assert_eq!(empty.cursor(), offset(0));
    assert_eq!(empty.document().generation(), 0);
    assert!(!empty.document().is_dirty());
}

#[test]
fn no_op_preserves_generation_and_redo_but_real_edit_replaces_redo() {
    let mut document = memory(b"abc");
    document.overwrite(offset(1), b"X").unwrap();
    document.undo().unwrap();
    let generation = document.generation();
    document.overwrite(offset(1), b"b").unwrap();
    document.overwrite(offset(3), b"").unwrap();
    assert_eq!(document.generation(), generation);
    assert!(document.redo().unwrap());
    document.undo().unwrap();
    document.overwrite(offset(2), b"Y").unwrap();
    assert!(!document.redo().unwrap());
    assert_eq!(logical(&mut document), b"abY");
}

#[test]
fn invalid_and_overflowing_ranges_leave_bytes_and_history_unchanged() {
    let mut document = memory(b"abc");
    document.overwrite(offset(1), b"X").unwrap();
    document.undo().unwrap();
    let generation = document.generation();
    for (position, bytes) in [(2, b"XY".as_slice()), (3, b"X"), (u64::MAX, b"XY")] {
        assert!(matches!(
            document.overwrite(offset(position), bytes),
            Err(Error::OutOfBounds { .. })
        ));
    }
    assert!(matches!(
        document.read_at(offset(4), &mut []),
        Err(Error::OutOfBounds { .. })
    ));
    assert_eq!(document.generation(), generation);
    assert_eq!(logical(&mut document), b"abc");
    assert!(document.redo().unwrap());
    assert_eq!(logical(&mut document), b"aXc");
}

#[test]
fn bounded_reads_return_eof_without_filling_unrequested_bytes() {
    let mut document = memory(b"abc");
    let mut buffer = [0xFF; 8];
    assert_eq!(document.read_at(offset(1), &mut buffer).unwrap(), 2);
    assert_eq!(buffer, [b'b', b'c', 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    assert_eq!(document.read_at(offset(3), &mut buffer).unwrap(), 0);
    let mut oversized = vec![0; MAX_READ_BYTES + 1];
    assert!(matches!(
        document.read_at(offset(0), &mut oversized),
        Err(Error::LimitExceeded { .. })
    ));
    assert!(matches!(
        document.overwrite(offset(0), &oversized),
        Err(Error::LimitExceeded { .. })
    ));
    assert!(!document.is_dirty());
}

#[test]
fn empty_document_supports_zero_position_and_rejects_append() {
    let mut document = memory(b"");
    assert!(document.is_empty());
    assert_eq!(document.read_at(offset(0), &mut [0; 1]).unwrap(), 0);
    document.overwrite(offset(0), &[]).unwrap();
    assert!(matches!(
        document.overwrite(offset(0), b"X"),
        Err(Error::OutOfBounds { .. })
    ));
    let mut editor = Editor::new(document, false);
    editor.dispatch(Command::Move(i64::MAX)).unwrap();
    editor.dispatch(Command::Goto(offset(u64::MAX))).unwrap();
    assert_eq!(editor.cursor(), offset(0));
    editor.dispatch(Command::BeginEdit).unwrap();
    assert!(editor.dispatch(Command::WriteByte(1)).is_err());
    assert_eq!(editor.cursor(), offset(0));
}

#[derive(Debug)]
struct FlakySource {
    bytes: MemorySource,
    successful_reads_left: Rc<Cell<Option<usize>>>,
    chunk: usize,
}

impl ByteSource for FlakySource {
    fn len(&self) -> u64 {
        self.bytes.len()
    }

    fn read_at(&mut self, position: FileOffset, out: &mut [u8]) -> io::Result<usize> {
        if let Some(left) = self.successful_reads_left.get() {
            if left == 0 {
                return Err(io::Error::other("injected source failure"));
            }
            self.successful_reads_left.set(Some(left - 1));
        }
        let count = out.len().min(self.chunk);
        self.bytes.read_at(position, &mut out[..count])
    }
}

#[test]
fn source_failure_after_short_read_does_not_change_overlay_or_redo() {
    let successful_reads_left = Rc::new(Cell::new(None));
    let mut document = Document::new(FlakySource {
        bytes: MemorySource::new(b"abcdef".to_vec()),
        successful_reads_left: Rc::clone(&successful_reads_left),
        chunk: 2,
    });
    document.overwrite(offset(1), b"XY").unwrap();
    document.undo().unwrap();
    let generation = document.generation();
    successful_reads_left.set(Some(1));
    assert!(matches!(
        document.overwrite(offset(0), b"1234"),
        Err(Error::Io(_))
    ));
    assert_eq!(document.generation(), generation);
    assert!(!document.is_dirty());
    assert!(document.patches().is_empty());
    successful_reads_left.set(None);
    assert_eq!(logical(&mut document), b"abcdef");
    assert!(document.redo().unwrap());
    assert_eq!(logical(&mut document), b"aXYdef");
    successful_reads_left.set(Some(0));
    // Undo/redo rely on stored deltas and cannot fail because the source fails.
    assert!(document.undo().unwrap());
    assert!(document.redo().unwrap());
}

#[test]
fn failed_restore_keeps_cursor_bytes_generation_and_redo() {
    let successful_reads_left = Rc::new(Cell::new(None));
    let source = FlakySource {
        bytes: MemorySource::new(b"abc".to_vec()),
        successful_reads_left: Rc::clone(&successful_reads_left),
        chunk: 1,
    };
    let mut editor = Editor::new(Document::new(source), false);
    editor.dispatch(Command::BeginEdit).unwrap();
    editor.dispatch(Command::WriteByte(b'X')).unwrap();
    editor.dispatch(Command::WriteByte(b'Y')).unwrap();
    editor.dispatch(Command::Undo).unwrap();
    editor.dispatch(Command::Goto(offset(1))).unwrap();
    let generation = editor.document().generation();
    successful_reads_left.set(Some(0));
    assert!(matches!(
        editor.dispatch(Command::RestorePreviousByte),
        Err(Error::Io(_))
    ));
    assert_eq!(editor.cursor(), offset(1));
    editor.dispatch(Command::Goto(offset(0))).unwrap();
    assert!(matches!(
        editor.dispatch(Command::RestoreByte),
        Err(Error::Io(_))
    ));
    assert_eq!(editor.cursor(), offset(0));
    assert_eq!(editor.document().generation(), generation);
    assert!(editor.document().is_dirty());
    successful_reads_left.set(None);
    assert_eq!(logical(editor.document_mut()), b"Xbc");
    editor.dispatch(Command::Redo).unwrap();
    assert_eq!(logical(editor.document_mut()), b"XYc");
}

#[derive(Debug)]
struct BadLengthSource {
    returned: usize,
}

impl ByteSource for BadLengthSource {
    fn len(&self) -> u64 {
        8
    }
    fn read_at(&mut self, _: FileOffset, _: &mut [u8]) -> io::Result<usize> {
        Ok(self.returned)
    }
}

#[test]
fn premature_eof_and_invalid_read_counts_are_errors_without_edits() {
    for (returned, kind) in [
        (0, io::ErrorKind::UnexpectedEof),
        (9, io::ErrorKind::InvalidData),
    ] {
        let mut document = Document::new(BadLengthSource { returned });
        assert!(
            matches!(document.overwrite(offset(0), b"XY"), Err(Error::Io(error)) if error.kind() == kind)
        );
        assert!(!document.is_dirty());
        assert_eq!(document.generation(), 0);
        assert!(!document.undo().unwrap());
    }
}

#[test]
fn rebase_clears_pending_history_and_adopts_persisted_bytes() {
    let mut document = memory(b"abc");
    document.overwrite(offset(1), b"X").unwrap();
    let generation = document.generation();
    document.rebase(MemorySource::new(b"aXc".to_vec()));
    assert_eq!(logical(&mut document), b"aXc");
    assert!(!document.is_dirty());
    assert!(document.generation() > generation);
    assert!(!document.undo().unwrap());
    assert!(!document.redo().unwrap());
    document.overwrite(offset(2), b"Y").unwrap();
    document.discard();
    assert_eq!(logical(&mut document), b"aXc");
}

#[test]
fn pending_byte_limit_rejects_atomically() {
    let mut document = Document::new(MemorySource::new(vec![0; MAX_PENDING_BYTES + 1]));
    let block = vec![1; MAX_EDIT_BYTES];
    for start in (0..MAX_PENDING_BYTES).step_by(MAX_EDIT_BYTES) {
        document.overwrite(offset(start as u64), &block).unwrap();
    }
    let generation = document.generation();
    assert!(matches!(
        document.overwrite(offset(MAX_PENDING_BYTES as u64), &[1]),
        Err(Error::LimitExceeded {
            resource: "pending edited bytes",
            ..
        })
    ));
    assert_eq!(document.generation(), generation);
    assert_eq!(
        document
            .patches()
            .iter()
            .map(|patch| patch.bytes.len())
            .sum::<usize>(),
        MAX_PENDING_BYTES
    );
    let mut byte = [9];
    document
        .read_at(offset(MAX_PENDING_BYTES as u64), &mut byte)
        .unwrap();
    assert_eq!(byte, [0]);
    assert!(document.undo().unwrap());
    document
        .overwrite(offset(MAX_PENDING_BYTES as u64), &[1])
        .unwrap();
}

#[test]
fn history_payload_limit_retains_the_previous_transaction() {
    let mut document = Document::new(MemorySource::new(vec![0; MAX_EDIT_BYTES]));
    let operations = MAX_HISTORY_BYTES / (2 * MAX_EDIT_BYTES * size_of::<Option<u8>>());
    for index in 0..operations {
        document
            .overwrite(offset(0), &vec![1 + (index % 2) as u8; MAX_EDIT_BYTES])
            .unwrap();
    }
    let generation = document.generation();
    let next = 1 + (operations % 2) as u8;
    assert!(matches!(
        document.overwrite(offset(0), &vec![next; MAX_EDIT_BYTES]),
        Err(Error::LimitExceeded {
            resource: "undo/redo payload bytes",
            ..
        })
    ));
    assert_eq!(document.generation(), generation);
    assert_eq!(
        logical(&mut document),
        vec![1 + ((operations - 1) % 2) as u8; MAX_EDIT_BYTES]
    );
    assert!(document.undo().unwrap());
    // A new branch replaces the undone record and fits the same bounded budget.
    document
        .overwrite(offset(0), &vec![3; MAX_EDIT_BYTES])
        .unwrap();
    assert!(!document.redo().unwrap());
}

#[test]
fn history_record_limit_does_not_destroy_undo_or_redo() {
    let mut document = memory(&[0]);
    for index in 0..MAX_HISTORY_ENTRIES {
        document
            .overwrite(offset(0), &[1 + (index % 2) as u8])
            .unwrap();
    }
    let generation = document.generation();
    assert!(matches!(
        document.overwrite(offset(0), &[3]),
        Err(Error::LimitExceeded {
            resource: "undo/redo command count",
            ..
        })
    ));
    assert_eq!(document.generation(), generation);
    assert!(matches!(
        document.restore_byte(offset(0)),
        Err(Error::LimitExceeded {
            resource: "undo/redo command count",
            ..
        })
    ));
    assert_eq!(document.generation(), generation);
    assert!(document.undo().unwrap());
    assert!(document.redo().unwrap());
    assert_eq!(logical(&mut document), [2]);
}

#[derive(Debug)]
struct HugeSource {
    len: u64,
    reads: usize,
    bytes_read: usize,
    largest_read: usize,
}

impl HugeSource {
    fn new(len: u64) -> Self {
        Self {
            len,
            reads: 0,
            bytes_read: 0,
            largest_read: 0,
        }
    }
}

impl ByteSource for HugeSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&mut self, position: FileOffset, out: &mut [u8]) -> io::Result<usize> {
        assert!(out.len() <= MAX_READ_BYTES);
        assert!(position.get() <= self.len);
        self.reads += 1;
        self.bytes_read += out.len();
        self.largest_read = self.largest_read.max(out.len());
        let count = out
            .len()
            .min((self.len - position.get()).min(MAX_READ_BYTES as u64) as usize);
        for (index, byte) in out[..count].iter_mut().enumerate() {
            *byte = position.get().wrapping_add(index as u64) as u8;
        }
        Ok(count)
    }
}

#[test]
fn huge_offsets_use_only_requested_bytes_and_undo_does_not_read_source() {
    let mut document = Document::new(HugeSource::new((1 << 34) + 17));
    assert_eq!(document.source().reads, 0);
    let position = (1 << 32) + 0xFE;
    let mut bytes = [0; 4];
    document.read_at(offset(position), &mut bytes).unwrap();
    assert_eq!(bytes, [0xFE, 0xFF, 0, 1]);
    document.overwrite(offset(position + 1), &[0x12]).unwrap();
    let reads = document.source().reads;
    document.undo().unwrap();
    document.redo().unwrap();
    assert_eq!(document.source().reads, reads);
    document.read_at(offset(position), &mut bytes).unwrap();
    assert_eq!(bytes, [0xFE, 0x12, 0, 1]);
    assert_eq!(document.source().bytes_read, 9);
    assert_eq!(document.source().largest_read, 4);
    assert_eq!(
        document.patches(),
        vec![Patch {
            offset: offset(position + 1),
            bytes: vec![0x12]
        }]
    );
}

#[test]
fn offsets_near_u64_max_are_checked_before_source_access() {
    let mut document = Document::new(HugeSource::new(u64::MAX));
    assert!(matches!(
        document.overwrite(offset(u64::MAX - 1), &[1, 2]),
        Err(Error::OutOfBounds { .. })
    ));
    assert_eq!(document.source().reads, 0);
    document.overwrite(offset(u64::MAX - 1), &[0]).unwrap();
    let mut byte = [99];
    assert_eq!(
        document.read_at(offset(u64::MAX - 1), &mut byte).unwrap(),
        1
    );
    assert_eq!(byte, [0]);
    assert_eq!(document.read_at(offset(u64::MAX), &mut byte).unwrap(), 0);
    document.restore_byte(offset(u64::MAX - 1)).unwrap();
    document.read_at(offset(u64::MAX - 1), &mut byte).unwrap();
    assert_eq!(byte, [0xFE]);
    assert!(!document.is_dirty());
}

#[test]
fn shared_commands_enforce_edit_state_read_only_and_cursor_rules() {
    let mut editor = Editor::new(memory(b"abc"), false);
    assert!(matches!(
        editor.dispatch(Command::WriteByte(b'X')),
        Err(Error::NotEditing)
    ));
    assert!(matches!(
        editor.dispatch(Command::Save),
        Err(Error::NotEditing)
    ));
    for command in [Command::RestoreByte, Command::RestorePreviousByte] {
        assert!(matches!(editor.dispatch(command), Err(Error::NotEditing)));
    }
    editor.dispatch(Command::Move(i64::MAX)).unwrap();
    assert_eq!(editor.cursor(), offset(2));
    editor.dispatch(Command::Move(i64::MIN)).unwrap();
    assert_eq!(editor.cursor(), offset(0));
    editor.dispatch(Command::Goto(offset(1))).unwrap();
    editor.dispatch(Command::CycleMode).unwrap();
    assert_eq!(editor.mode(), ViewMode::Code);
    let navigation = FileNavigation::History;
    assert_eq!(
        editor
            .dispatch(Command::FileNavigation(navigation))
            .unwrap(),
        Effect::FileNavigation(navigation)
    );
    assert_eq!(editor.cursor(), offset(1));
    editor.dispatch(Command::BeginEdit).unwrap();
    editor.dispatch(Command::WriteByte(b'X')).unwrap();
    assert!(matches!(
        editor.dispatch(Command::FileNavigation(navigation)),
        Err(Error::EditingInProgress)
    ));
    assert_eq!(editor.cursor(), offset(2));
    editor.dispatch(Command::WriteByte(b'Y')).unwrap();
    assert_eq!(editor.cursor(), offset(2));
    editor.dispatch(Command::BeginEdit).unwrap();
    assert_eq!(logical(editor.document_mut()), b"aXY");
    assert_eq!(editor.dispatch(Command::Save).unwrap(), Effect::Save);
    assert!(editor.is_editing());
    assert!(editor.document().is_dirty());
    editor.complete_save(MemorySource::new(b"aXY".to_vec()));
    assert!(!editor.is_editing());
    assert!(!editor.document().is_dirty());
    editor.dispatch(Command::BeginEdit).unwrap();
    editor.dispatch(Command::WriteByte(b'Z')).unwrap();
    editor.dispatch(Command::DiscardEdit).unwrap();
    assert_eq!(logical(editor.document_mut()), b"aXY");
    assert!(!editor.is_editing());

    let mut read_only = Editor::new(memory(b"abc"), true);
    for command in [
        Command::BeginEdit,
        Command::WriteByte(0),
        Command::RestoreByte,
        Command::RestorePreviousByte,
        Command::Undo,
        Command::Redo,
        Command::Save,
    ] {
        assert!(matches!(read_only.dispatch(command), Err(Error::ReadOnly)));
    }
    assert!(read_only.is_read_only());
    assert!(!read_only.document().is_dirty());
}

#[test]
fn command_registry_contains_every_stable_command_once() {
    let structure = crate::formats::StructureRef {
        generation: 0,
        slice: crate::formats::FileRange {
            offset: offset(0),
            len: 0,
        },
        selected_slice: None,
        index: 0,
    };
    let commands = [
        Command::BeginEdit,
        Command::DiscardEdit,
        Command::WriteByte(0),
        Command::RestoreByte,
        Command::RestorePreviousByte,
        Command::Undo,
        Command::Redo,
        Command::ToggleBlock,
        Command::ClearBlock,
        Command::SelectWholeBlock,
        Command::ResizeBlock,
        Command::GotoBlockBoundary(BlockBoundary::First),
        Command::ExportBlock,
        Command::OpenCalculator,
        Command::Bookmark(BookmarkCommand::Store(BookmarkViewport::default())),
        Command::Bookmark(BookmarkCommand::Recall),
        Command::Bookmark(BookmarkCommand::Select(1)),
        Command::Bookmark(BookmarkCommand::Delete),
        Command::Bookmark(BookmarkCommand::Clear),
        Command::Move(1),
        Command::Goto(offset(0)),
        Command::GotoVirtual(crate::formats::VirtualAddress::new(0)),
        Command::GotoEntry,
        Command::GotoRegion(0),
        Command::GotoSymbol(0),
        Command::GotoSymbolRecord(0),
        Command::GotoStructure(structure),
        Command::GotoStructureRecord(structure),
        Command::SelectSlice(0),
        Command::SetDecodeMode(None),
        Command::SetEndian(None),
        Command::SetRawBase(crate::formats::VirtualAddress::new(0)),
        Command::SetRaw(false),
        Command::FindReferences,
        Command::NextReference,
        Command::ToggleReferenceAlignment,
        Command::FollowBranch(1),
        Command::ReturnBranch,
        Command::MoveCode(1),
        Command::ResyncCode,
        Command::SetMode(ViewMode::Hex),
        Command::CycleMode,
        Command::BrowseFiles,
        Command::FileNavigation(FileNavigation::History),
        Command::FileNavigation(FileNavigation::NextRecent),
        Command::FileNavigation(FileNavigation::Arguments),
        Command::FileNavigation(FileNavigation::PreviousArgument),
        Command::FileNavigation(FileNavigation::NextArgument),
        Command::Save,
    ];
    assert_eq!(commands.len(), COMMANDS.len());
    for command in commands {
        assert_eq!(
            COMMANDS
                .iter()
                .filter(|info| info.id == command.id())
                .count(),
            1
        );
    }
    assert_eq!(Command::WriteByte(0xFF).id(), "edit.write-byte");
    assert_eq!(Command::Save.id(), "file.save");
}

#[test]
fn goto_supports_hex_decimal_relative_and_empty_file_zero() {
    for (input, current, expected) in [
        ("10", 0, 16),
        ("10t", 0, 10),
        ("0xFf", 0, 255),
        ("+10", 32, 48),
        ("-10", 32, 16),
        ("+10t", 32, 42),
        ("-10t", 32, 22),
        (" 0X20 ", 0, 32),
        ("15T", 0, 15),
    ] {
        assert_eq!(
            parse_goto(input, offset(current), 1024).unwrap(),
            offset(expected),
            "{input}"
        );
    }
    assert_eq!(parse_goto("0", offset(0), 0).unwrap(), offset(0));
}

#[test]
fn goto_rejects_ambiguity_overflow_underflow_eof_and_unmapped_virtual_addresses() {
    for input in [
        "",
        "+",
        "-",
        "0x",
        "t",
        "1 0",
        "++1",
        "-+1",
        "0x10t",
        "-1",
        "10000000000000000",
        "18446744073709551616t",
    ] {
        assert!(parse_goto(input, offset(0), u64::MAX).is_err(), "{input}");
    }
    assert!(parse_goto("+2", offset(u64::MAX - 1), u64::MAX).is_err());
    assert!(matches!(
        parse_goto("20", offset(0), 32),
        Err(Error::OutOfBounds { .. })
    ));
    assert!(matches!(
        parse_goto("1", offset(0), 0),
        Err(Error::OutOfBounds { .. })
    ));
    assert!(matches!(
        parse_goto(".401000", offset(0), 1024),
        Err(Error::UnsupportedAddress)
    ));
    assert!(matches!(
        parse_goto("10.20", offset(0), 1024),
        Err(Error::UnsupportedAddress)
    ));
}

#[test]
fn incremental_search_finds_cross_chunk_match_without_losing_prefix() {
    let mut bytes = vec![0; MAX_READ_BYTES + 10];
    bytes[MAX_READ_BYTES - 2..MAX_READ_BYTES + 4].copy_from_slice(b"ABCDEF");
    let mut document = Document::new(MemorySource::new(bytes));
    let mut search = Search::new(b"ABCDEF".to_vec(), offset(0), document.generation()).unwrap();
    assert_eq!(
        search.step(&mut document, usize::MAX).unwrap(),
        SearchProgress::Pending {
            next: offset(MAX_READ_BYTES as u64)
        }
    );
    assert_eq!(
        search.step(&mut document, 2).unwrap(),
        SearchProgress::Pending {
            next: offset(MAX_READ_BYTES as u64 + 2)
        }
    );
    let found = SearchProgress::Found(offset(MAX_READ_BYTES as u64 - 2));
    assert_eq!(search.step(&mut document, 2).unwrap(), found);
    assert_eq!(search.step(&mut document, 1).unwrap(), found);
}

#[test]
fn search_handles_overlapping_prefixes_and_pending_overlay() {
    let mut document = memory(b"aaaaacaaaac");
    document.overwrite(offset(5), b"b").unwrap();
    let mut search = Search::new(b"aaaab".to_vec(), offset(0), document.generation()).unwrap();
    assert_eq!(
        search.step(&mut document, 2).unwrap(),
        SearchProgress::Pending { next: offset(2) }
    );
    assert_eq!(
        search.step(&mut document, 2).unwrap(),
        SearchProgress::Pending { next: offset(4) }
    );
    assert_eq!(
        search.step(&mut document, 2).unwrap(),
        SearchProgress::Found(offset(1))
    );
}

#[test]
fn search_rejects_empty_oversized_patterns_and_stale_results() {
    assert!(matches!(
        Search::new(vec![], offset(0), 0),
        Err(Error::InvalidPattern(_))
    ));
    assert!(matches!(
        Search::new(vec![0; MAX_SEARCH_PATTERN_BYTES + 1], offset(0), 0),
        Err(Error::LimitExceeded { .. })
    ));
    let mut document = memory(b"abcd");
    let mut search = Search::new(b"a".to_vec(), offset(0), document.generation()).unwrap();
    assert_eq!(
        search.step(&mut document, 1).unwrap(),
        SearchProgress::Found(offset(0))
    );
    document.overwrite(offset(0), b"A").unwrap();
    assert!(matches!(
        search.step(&mut document, 1),
        Err(Error::StaleSearch { .. })
    ));
    let mut search = Search::new(b"A".to_vec(), offset(0), document.generation()).unwrap();
    document.discard();
    assert!(matches!(
        search.step(&mut document, 1),
        Err(Error::StaleSearch { .. })
    ));
}

#[test]
fn search_eof_zero_budget_and_cancellation_have_no_document_effect() {
    let mut document = memory(b"abcd");
    let mut search = Search::new(b"z".to_vec(), offset(0), document.generation()).unwrap();
    assert_eq!(
        search.step(&mut document, 0).unwrap(),
        SearchProgress::Pending { next: offset(0) }
    );
    assert_eq!(
        search.step(&mut document, 2).unwrap(),
        SearchProgress::Pending { next: offset(2) }
    );
    drop(search);
    assert!(!document.is_dirty());
    assert_eq!(document.generation(), 0);
    let mut search = Search::new(b"z".to_vec(), offset(0), document.generation()).unwrap();
    assert_eq!(
        search.step(&mut document, 99).unwrap(),
        SearchProgress::NotFound
    );
    assert_eq!(
        search.step(&mut document, 1).unwrap(),
        SearchProgress::NotFound
    );
    let mut at_eof = Search::new(b"a".to_vec(), offset(4), document.generation()).unwrap();
    assert_eq!(
        at_eof.step(&mut document, 0).unwrap(),
        SearchProgress::NotFound
    );
    let mut past_eof = Search::new(b"a".to_vec(), offset(5), document.generation()).unwrap();
    assert!(matches!(
        past_eof.step(&mut document, 1),
        Err(Error::OutOfBounds { .. })
    ));
}

#[test]
fn search_at_large_offsets_and_budget_cap_never_reads_whole_file() {
    let boundary = 1 << 32;
    let mut document = Document::new(HugeSource::new(1 << 34));
    let mut search = Search::new(
        vec![0xFE, 0xFF, 0, 1],
        offset(boundary - 3),
        document.generation(),
    )
    .unwrap();
    assert_eq!(
        search.step(&mut document, 2).unwrap(),
        SearchProgress::Pending {
            next: offset(boundary - 1)
        }
    );
    assert_eq!(
        search.step(&mut document, 3).unwrap(),
        SearchProgress::Found(offset(boundary - 2))
    );
    assert_eq!(document.source().bytes_read, 5);
    let mut missing = Search::new(vec![1, 1], offset(0), document.generation()).unwrap();
    assert_eq!(
        missing.step(&mut document, usize::MAX).unwrap(),
        SearchProgress::Pending {
            next: offset(MAX_READ_BYTES as u64)
        }
    );
    assert_eq!(document.source().largest_read, MAX_READ_BYTES);
}

#[test]
fn search_source_failure_preserves_progress_for_a_retry() {
    let successful_reads_left = Rc::new(Cell::new(None));
    let mut document = Document::new(FlakySource {
        bytes: MemorySource::new(b"xxABCyy".to_vec()),
        successful_reads_left: Rc::clone(&successful_reads_left),
        chunk: 2,
    });
    let mut search = Search::new(b"ABC".to_vec(), offset(0), document.generation()).unwrap();
    assert_eq!(
        search.step(&mut document, 3).unwrap(),
        SearchProgress::Pending { next: offset(3) }
    );
    successful_reads_left.set(Some(1));
    assert!(matches!(search.step(&mut document, 4), Err(Error::Io(_))));
    successful_reads_left.set(None);
    assert_eq!(
        search.step(&mut document, 2).unwrap(),
        SearchProgress::Found(offset(2))
    );
}

#[test]
fn scoped_literal_search_matches_reference_windows_at_each_budget_and_direction() {
    let bytes = b"AaaaAAaBA";
    let mut document = memory(bytes);
    for direction in [SearchDirection::Forward, SearchDirection::Backward] {
        for case in [SearchCase::Exact, SearchCase::AsciiInsensitive] {
            for even in [false, true] {
                for first in 0..=bytes.len() {
                    for end in first..=bytes.len() {
                        for start in 0..=bytes.len() {
                            let mut candidates = bytes[first..end]
                                .windows(3)
                                .enumerate()
                                .map(|(delta, window)| (first + delta, window))
                                .filter(|(at, window)| {
                                    (!even || at % 2 == 0)
                                        && match case {
                                            SearchCase::Exact => *window == b"aAA",
                                            SearchCase::AsciiInsensitive => {
                                                window.eq_ignore_ascii_case(b"aAA")
                                            }
                                        }
                                })
                                .map(|(at, _)| at);
                            let expected = match direction {
                                SearchDirection::Forward => candidates.find(|at| *at >= start),
                                SearchDirection::Backward => candidates.rfind(|at| *at <= start),
                            }
                            .map_or(SearchProgress::NotFound, |at| {
                                SearchProgress::Found(offset(at as u64))
                            });
                            let mut request = SearchRequest::utf8("aAA".into(), case).unwrap();
                            request.direction = direction;
                            request.even = even;
                            request.scope = Some(offset(first as u64)..offset(end as u64));
                            for budget in [1, 2, usize::MAX] {
                                let mut search =
                                    Search::from_request(&request, offset(start as u64), 0);
                                let actual = (0..=bytes.len())
                                    .map(|_| search.step(&mut document, budget).unwrap())
                                    .find(|progress| {
                                        !matches!(progress, SearchProgress::Pending { .. })
                                    })
                                    .expect("scan terminates within the fixture length");
                                assert_eq!(
                                    actual, expected,
                                    "{request:?}, origin {start}, budget {budget}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn reverse_search_retains_prefix_across_bounded_reads_at_large_offsets() {
    let boundary = 1 << 32;
    let mut document = Document::new(HugeSource::new(u64::MAX));
    let mut request = SearchRequest::bytes(vec![0xFE, 0xFF, 0, 1]).unwrap();
    request.direction = SearchDirection::Backward;
    request.scope = Some(offset(boundary - 5)..offset(boundary + 5));
    let mut search = Search::from_request(&request, offset(boundary + 4), document.generation());
    assert_eq!(
        search.step(&mut document, 0).unwrap(),
        SearchProgress::Pending {
            next: offset(boundary + 5)
        }
    );
    assert_eq!(
        search.step(&mut document, 4).unwrap(),
        SearchProgress::Pending {
            next: offset(boundary + 1)
        }
    );
    assert_eq!(
        search.step(&mut document, 4).unwrap(),
        SearchProgress::Found(offset(boundary - 2))
    );
    assert_eq!(document.source().bytes_read, 8);

    request.scope = None;
    let mut near_end = Search::from_request(&request, offset(u64::MAX - 2), document.generation());
    assert_eq!(
        near_end.step(&mut document, 3).unwrap(),
        SearchProgress::Pending {
            next: offset(u64::MAX - 3)
        }
    );
    assert_eq!(document.source().largest_read, 4);
}

#[test]
fn search_resume_survives_canceled_replacement_and_resets_after_edit() {
    let mut editor = Editor::new(memory(b"a----a----a"), false);
    editor.start_search(SearchRequest::bytes(vec![b'a']).unwrap());
    assert_eq!(
        editor.step_search(99).unwrap(),
        Some(SearchProgress::Found(offset(0)))
    );
    editor.dispatch(Command::Move(7)).unwrap();
    editor.start_search(SearchRequest::bytes(vec![b'b']).unwrap());
    assert!(matches!(
        editor.step_search(1).unwrap(),
        Some(SearchProgress::Pending { .. })
    ));
    editor.cancel_search();
    assert_eq!(editor.cursor(), offset(7));
    assert_eq!(editor.last_search_hit(), Some(offset(0)));
    assert_eq!(editor.repeat_search(), SearchRepeat::Started);
    assert_eq!(
        editor.step_search(99).unwrap(),
        Some(SearchProgress::Found(offset(5)))
    );
    assert_eq!(editor.cursor(), offset(5));

    editor.dispatch(Command::BeginEdit).unwrap();
    editor.dispatch(Command::WriteByte(b'x')).unwrap();
    assert_eq!(editor.last_search_hit(), None);
    editor.goto_expression("0").unwrap();
    assert_eq!(editor.repeat_search(), SearchRepeat::Started);
    assert_eq!(
        editor.step_search(99).unwrap(),
        Some(SearchProgress::Found(offset(0)))
    );
    assert_eq!(logical(editor.document_mut()), b"a----x----a");
    assert!(editor.document().is_dirty());
}

#[test]
fn block_tracks_reverse_and_crossing_motion_then_freezes_inclusive_boundaries() {
    let mut editor = Editor::new(memory(b"0123456789"), true);
    editor.dispatch(Command::Goto(offset(5))).unwrap();
    editor.dispatch(Command::ToggleBlock).unwrap();
    let block = editor.block_selection().unwrap();
    assert!(block.is_marking);
    assert_eq!((block.range.offset, block.range.len), (offset(5), 1));
    editor.dispatch(Command::Move(-3)).unwrap();
    let block = editor.block_selection().unwrap();
    assert_eq!((block.range.offset, block.range.len), (offset(2), 4));
    editor.dispatch(Command::Move(5)).unwrap();
    let block = editor.block_selection().unwrap();
    assert_eq!((block.range.offset, block.range.len), (offset(5), 3));

    assert!(matches!(
        editor.dispatch(Command::ExportBlock),
        Err(Error::Block(_))
    ));
    editor.dispatch(Command::ToggleBlock).unwrap();
    editor.dispatch(Command::Move(-7)).unwrap();
    let block = editor.block_selection().unwrap();
    assert!(!block.is_marking);
    assert_eq!((block.range.offset, block.range.len), (offset(5), 3));
    assert_eq!(
        editor.dispatch(Command::ExportBlock).unwrap(),
        Effect::ExportBlock(block.range)
    );
    editor
        .dispatch(Command::GotoBlockBoundary(BlockBoundary::Last))
        .unwrap();
    assert_eq!(editor.cursor(), offset(7));
    editor
        .dispatch(Command::GotoBlockBoundary(BlockBoundary::First))
        .unwrap();
    assert_eq!(editor.cursor(), offset(5));
    editor.dispatch(Command::Goto(offset(1))).unwrap();
    editor.dispatch(Command::ResizeBlock).unwrap();
    let resized = editor.block_selection().unwrap().range;
    assert_eq!((resized.offset, resized.len), (offset(1), 5));
    editor.dispatch(Command::ToggleBlock).unwrap();
    assert!(editor.block_selection().is_none());
    assert_eq!(editor.document().generation(), 0);
    assert!(!editor.document().is_dirty());
}

#[test]
fn block_select_all_can_span_the_full_u64_file_domain_without_reads() {
    let mut empty = Editor::new(memory(b""), true);
    assert!(matches!(
        empty.dispatch(Command::ToggleBlock),
        Err(Error::Block(_))
    ));
    assert!(matches!(
        empty.dispatch(Command::SelectWholeBlock),
        Err(Error::Block(_))
    ));
    assert!(empty.block_selection().is_none());

    let mut editor = Editor::new(Document::new(HugeSource::new(u64::MAX)), true);
    editor.dispatch(Command::SelectWholeBlock).unwrap();
    let range = editor.block_selection().unwrap().range;
    assert_eq!(range.offset, offset(0));
    assert_eq!(range.end().unwrap(), offset(u64::MAX));
    editor
        .dispatch(Command::GotoBlockBoundary(BlockBoundary::Last))
        .unwrap();
    assert_eq!(editor.cursor(), offset(u64::MAX - 1));
    assert_eq!(editor.document().source().reads, 0);
}

#[test]
fn editor_retains_a_fixed_block_across_mode_and_edit_transactions() {
    let mut editor = Editor::new(memory(b"01234"), false);
    editor.dispatch(Command::ToggleBlock).unwrap();
    editor.dispatch(Command::Move(2)).unwrap();
    editor.dispatch(Command::SetMode(ViewMode::Text)).unwrap();
    let block = editor.block_selection().unwrap();
    assert!(!block.is_marking);
    assert!(matches!(
        editor.dispatch(Command::ToggleBlock),
        Err(Error::Block(_))
    ));
    editor.dispatch(Command::Move(1)).unwrap();
    editor.dispatch(Command::SetMode(ViewMode::Code)).unwrap();
    editor.dispatch(Command::BeginEdit).unwrap();
    editor.dispatch(Command::WriteByte(b'X')).unwrap();
    assert_eq!(editor.block_selection(), Some(block));
    editor.dispatch(Command::DiscardEdit).unwrap();
    assert_eq!(editor.block_selection(), Some(block));
    assert_eq!(logical(editor.document_mut()), b"01234");
    editor.complete_save(MemorySource::new(vec![0]));
    assert!(editor.block_selection().is_none());
}

#[test]
fn browsing_files_preserves_the_document_and_is_unavailable_during_editing() {
    let mut editor = Editor::new(memory(b"abc"), false);
    editor.dispatch(Command::Goto(offset(1))).unwrap();
    assert_eq!(
        editor.dispatch(Command::BrowseFiles).unwrap(),
        Effect::BrowseFiles
    );
    assert_eq!(editor.cursor(), offset(1));
    editor.dispatch(Command::BeginEdit).unwrap();
    editor.dispatch(Command::WriteByte(b'X')).unwrap();
    assert!(matches!(
        editor.dispatch(Command::BrowseFiles),
        Err(Error::EditingInProgress)
    ));
    assert!(editor.is_editing());
    assert_eq!(logical(editor.document_mut()), b"aXc");
    assert_eq!(editor.dispatch(Command::Save).unwrap(), Effect::Save);
}
