use crate::{FileOffset, formats::FileRange};

/// Which inclusive endpoint a block navigation command targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockBoundary {
    First,
    Last,
}

/// A block with both marked bytes included in the file range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockSelection {
    pub range: FileRange,
    pub is_marking: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BlockMark {
    Tracking(FileOffset),
    Fixed {
        anchor: FileOffset,
        edge: FileOffset,
    },
}

impl BlockMark {
    pub(crate) fn selection(self, cursor: FileOffset, len: u64) -> Option<BlockSelection> {
        let (anchor, edge) = match self {
            Self::Tracking(anchor) => (anchor, cursor),
            Self::Fixed { anchor, edge } => (anchor, edge),
        };
        let first = anchor.min(edge);
        let last = anchor.max(edge);
        if last.get() >= len {
            return None;
        }
        Some(BlockSelection {
            range: FileRange {
                offset: first,
                len: last.get() - first.get() + 1,
            },
            is_marking: matches!(self, Self::Tracking(_)),
        })
    }

    pub(crate) fn finish(self, cursor: FileOffset) -> Self {
        match self {
            Self::Tracking(anchor) => Self::Fixed {
                anchor,
                edge: cursor,
            },
            fixed => fixed,
        }
    }

    pub(crate) fn resize(self, cursor: FileOffset) -> Self {
        let anchor = match self {
            Self::Tracking(anchor) | Self::Fixed { anchor, .. } => anchor,
        };
        Self::Fixed {
            anchor,
            edge: cursor,
        }
    }
}
