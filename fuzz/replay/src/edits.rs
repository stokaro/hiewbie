use hiewbie_core::{
    Command, Document, Editor, Effect, FileOffset, MAX_EDIT_BYTES, MemorySource, ViewMode,
};

/// Independent oracle: full snapshots of at most 64 bytes, rather than the
/// production sparse overlay and partial-history representation.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Model {
    original: Vec<u8>,
    bytes: Vec<u8>,
    undo: Vec<Vec<u8>>,
    redo: Vec<Vec<u8>>,
    cursor: usize,
    editing: bool,
    read_only: bool,
    generation: u64,
    mode: ViewMode,
}

impl Model {
    fn replace(&mut self, at: usize, value: u8) {
        if self.bytes[at] != value {
            self.undo.push(self.bytes.clone());
            self.redo.clear();
            self.bytes[at] = value;
            self.generation += 1;
        }
    }

    fn discard(&mut self) {
        self.generation += u64::from(self.bytes != self.original);
        self.bytes.clone_from(&self.original);
        self.undo.clear();
        self.redo.clear();
    }

    fn apply(&mut self, command: Command) -> bool {
        match command {
            Command::BeginEdit => {
                if self.read_only {
                    return false;
                }
                if !self.editing {
                    self.discard();
                    self.editing = true;
                }
            }
            Command::DiscardEdit => {
                if self.editing {
                    self.discard();
                    self.editing = false;
                }
            }
            Command::WriteByte(_)
            | Command::RestoreByte
            | Command::RestorePreviousByte
            | Command::Undo
            | Command::Redo
            | Command::Save
                if !self.editing || self.read_only =>
            {
                return false;
            }
            Command::WriteByte(value) => {
                if self.bytes.is_empty() {
                    return false;
                }
                self.replace(self.cursor, value);
                self.cursor = (self.cursor + 1).min(self.bytes.len() - 1);
            }
            Command::RestoreByte => {
                if self.bytes.is_empty() {
                    return false;
                }
                self.replace(self.cursor, self.original[self.cursor]);
            }
            Command::RestorePreviousByte => {
                if self.cursor != 0 {
                    self.cursor -= 1;
                    self.replace(self.cursor, self.original[self.cursor]);
                }
            }
            Command::Undo => {
                if let Some(previous) = self.undo.pop() {
                    self.redo.push(std::mem::replace(&mut self.bytes, previous));
                    self.generation += 1;
                }
            }
            Command::Redo => {
                if let Some(next) = self.redo.pop() {
                    self.undo.push(std::mem::replace(&mut self.bytes, next));
                    self.generation += 1;
                }
            }
            Command::Goto(offset) => {
                self.cursor = offset.get().min(self.bytes.len().saturating_sub(1) as u64) as usize;
            }
            Command::Move(delta) => {
                self.cursor = (self.cursor as i128 + i128::from(delta))
                    .clamp(0, self.bytes.len().saturating_sub(1) as i128)
                    as usize;
            }
            Command::BrowseFiles if self.editing => return false,
            Command::SetMode(mode) => self.mode = mode,
            Command::Save | Command::BrowseFiles => {}
            _ => panic!("operation is outside the bounded edit model"),
        }
        true
    }

    fn verify(&self, editor: &mut Editor<MemorySource>) {
        let mut bytes = vec![0; self.bytes.len()];
        assert_eq!(
            editor
                .document_mut()
                .read_at(FileOffset::new(0), &mut bytes)
                .unwrap(),
            bytes.len()
        );
        assert_eq!(bytes, self.bytes);
        assert_eq!(editor.document().source().as_slice(), self.original);
        assert_eq!(editor.document().generation(), self.generation);
        assert_eq!(editor.document().is_dirty(), self.bytes != self.original);
        assert_eq!(editor.cursor().get(), self.cursor as u64);
        assert_eq!(editor.is_editing(), self.editing);
        assert_eq!(editor.is_read_only(), self.read_only);
        assert_eq!(editor.mode(), self.mode);
        let mut patched = self.original.clone();
        let mut previous_end = 0;
        for patch in editor.document().patches() {
            let start = patch.offset.get() as usize;
            assert!(start >= previous_end);
            previous_end = start + patch.bytes.len();
            patched[start..previous_end].copy_from_slice(&patch.bytes);
        }
        assert_eq!(patched, self.bytes);
        for (at, (&logical, &original)) in self.bytes.iter().zip(&self.original).enumerate() {
            assert_eq!(
                editor
                    .document()
                    .is_byte_modified(FileOffset::new(at as u64)),
                logical != original
            );
        }
    }
}

fn dispatch(model: &mut Model, editor: &mut Editor<MemorySource>, command: Command) {
    let before = model.clone();
    let expected = model.apply(command);
    let actual = editor.dispatch(command);
    assert_eq!(actual.is_ok(), expected, "{command:?}: {actual:?}");
    if !expected {
        assert_eq!(*model, before, "rejected operation changed the oracle");
    }
    if expected && command == Command::Save {
        assert_eq!(actual.unwrap(), Effect::Save);
    }
    model.verify(editor);
}

pub(crate) fn check(data: &[u8]) {
    let read_only = data.first().copied().unwrap_or(0) & 1 != 0;
    let body = data.get(2..).unwrap_or_default();
    let length = usize::from(data.get(1).copied().unwrap_or(0) % 65).min(body.len());
    let (bytes, operations) = body.split_at(length);
    let mut model = Model {
        original: bytes.to_vec(),
        bytes: bytes.to_vec(),
        undo: Vec::new(),
        redo: Vec::new(),
        cursor: 0,
        editing: false,
        read_only,
        generation: 0,
        mode: ViewMode::Hex,
    };
    let mut editor = Editor::new(Document::new(MemorySource::new(bytes.to_vec())), read_only);
    for op in operations.chunks(3).take(128) {
        let first = op.get(1).copied().unwrap_or(0);
        let second = op.get(2).copied().unwrap_or(0);
        let command = match op[0] % 14 {
            0 => Command::BeginEdit,
            1 => Command::DiscardEdit,
            2 => Command::WriteByte(first),
            3 => Command::RestoreByte,
            4 => Command::Undo,
            5 => Command::Redo,
            6 => Command::Goto(FileOffset::new(if second == 255 {
                u64::MAX
            } else {
                u64::from(u16::from_le_bytes([first, second]))
            })),
            7 => Command::Move(i64::from(i16::from_le_bytes([first, second]))),
            8 => Command::RestorePreviousByte,
            9 => Command::Save,
            10 => Command::BrowseFiles,
            11 => Command::SetMode(
                [ViewMode::Hex, ViewMode::Text, ViewMode::Code][usize::from(first % 3)],
            ),
            12 | 13 => {
                // Direct callers also reject range overflow and oversized
                // payloads without touching bytes, cursor or retained history.
                let replacement = if op[0] % 14 == 13 {
                    vec![first; MAX_EDIT_BYTES + 1]
                } else {
                    vec![first; 2]
                };
                let offset = if op[0] % 14 == 13 { 0 } else { u64::MAX };
                assert!(
                    editor
                        .document_mut()
                        .overwrite(FileOffset::new(offset), &replacement)
                        .is_err()
                );
                model.verify(&mut editor);
                continue;
            }
            _ => unreachable!(),
        };
        let was_editing = model.editing;
        dispatch(&mut model, &mut editor, command);
        if command == Command::Save && was_editing && first & 1 != 0 {
            // Simulate completed persistence without touching a filesystem.
            editor.complete_save(MemorySource::new(model.bytes.clone()));
            model.original.clone_from(&model.bytes);
            model.undo.clear();
            model.redo.clear();
            model.editing = false;
            model.generation += 1;
            model.verify(&mut editor);
        }
    }
    if model.editing {
        for _ in 0..=model.undo.len() {
            dispatch(&mut model, &mut editor, Command::Undo);
        }
        for _ in 0..=model.redo.len() {
            dispatch(&mut model, &mut editor, Command::Redo);
        }
        dispatch(&mut model, &mut editor, Command::DiscardEdit);
    }
    model.verify(&mut editor);
}
