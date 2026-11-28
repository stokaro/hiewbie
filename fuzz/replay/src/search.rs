use std::{cell::Cell, rc::Rc};

use hiewbie_core::{
    Document, FileOffset, MAX_READ_BYTES, MAX_SEARCH_PATTERN_BYTES, Search, SearchCase,
    SearchDirection, SearchProgress, SearchRequest,
};

use crate::{source::Source, word};

pub(crate) fn check(data: &[u8]) {
    let flags = data.first().copied().unwrap_or(0);
    let body = data.get(14..).unwrap_or_default();
    let width = usize::from(data.get(1).copied().unwrap_or(0)).min(body.len());
    let (pattern, bytes) = body.split_at(width);
    let mut expected_pattern = pattern.to_vec();
    let mut request = if flags & 2 == 0 {
        SearchRequest::bytes(expected_pattern.clone())
    } else {
        let text = String::from_utf8_lossy(pattern).into_owned();
        expected_pattern = text.as_bytes().to_vec();
        expected_pattern.make_ascii_lowercase();
        SearchRequest::utf8(text, SearchCase::AsciiInsensitive)
    };
    if flags & 128 != 0 {
        request = SearchRequest::bytes(vec![0; MAX_SEARCH_PATTERN_BYTES + 1]);
        assert!(request.is_err());
        return;
    }
    let Ok(mut request) = request else {
        assert!(expected_pattern.is_empty());
        return;
    };
    request.direction = if flags & 1 == 0 {
        SearchDirection::Forward
    } else {
        SearchDirection::Backward
    };
    request.even = flags & 4 != 0;
    let first = u16::from_le_bytes([
        data.get(10).copied().unwrap_or(0),
        data.get(11).copied().unwrap_or(0),
    ]) as usize;
    let end = u16::from_le_bytes([
        data.get(12).copied().unwrap_or(0),
        data.get(13).copied().unwrap_or(0),
    ]) as usize;
    if flags & 8 != 0 {
        request.scope = Some(FileOffset::new(first as u64)..FileOffset::new(end as u64));
    }
    let start = word(data, 2);
    let read = Rc::new(Cell::new(0));
    let limit = Rc::new(Cell::new(MAX_READ_BYTES));
    let mut document = Document::new(Source {
        bytes,
        len: bytes.len() as u64,
        short: 1 + usize::from(flags % 7),
        read: read.clone(),
        limit: limit.clone(),
    });
    let mut logical = bytes.to_vec();
    if flags & 32 != 0 && !logical.is_empty() {
        logical[0] ^= 0x20;
        document
            .overwrite(FileOffset::new(0), &logical[..1])
            .unwrap();
    }
    let original_generation = document.generation();
    let original_patches = document.patches();
    let mut scan = Search::from_request(&request, FileOffset::new(start), original_generation);
    if start > bytes.len() as u64 || (request.scope.is_some() && (first > end || end > bytes.len()))
    {
        read.set(0);
        assert!(scan.step(&mut document, 1).is_err());
        assert_eq!(read.get(), 0);
    } else {
        let (first, end) = if request.scope.is_some() {
            (first, end)
        } else {
            (0, bytes.len())
        };
        let mut matches = Vec::new();
        for at in first..end {
            if at + expected_pattern.len() > end || (request.even && at % 2 != 0) {
                continue;
            }
            let mut candidate = logical[at..at + expected_pattern.len()].to_vec();
            if flags & 2 != 0 {
                candidate.make_ascii_lowercase();
            }
            if candidate == expected_pattern
                && match request.direction {
                    SearchDirection::Forward => at as u64 >= start,
                    SearchDirection::Backward => at as u64 <= start,
                }
            {
                matches.push(at);
            }
        }
        let expected = if request.direction == SearchDirection::Forward {
            matches.first()
        } else {
            matches.last()
        }
        .map_or(SearchProgress::NotFound, |at| {
            SearchProgress::Found(FileOffset::new(*at as u64))
        });
        read.set(0);
        let mut progress = scan.step(&mut document, 0).unwrap();
        assert_eq!(read.get(), 0);
        let budget = 1 + usize::from(flags % 31);
        for _ in 0..=bytes.len() {
            if !matches!(progress, SearchProgress::Pending { .. }) {
                break;
            }
            read.set(0);
            limit.set(budget);
            let previous = progress;
            progress = scan.step(&mut document, budget).unwrap();
            assert!(read.get() <= budget);
            if matches!(progress, SearchProgress::Pending { .. }) {
                assert_ne!(progress, previous, "nonzero-budget search made no progress");
            }
        }
        assert_eq!(progress, expected);
        read.set(0);
        assert_eq!(
            scan.step(&mut document, MAX_READ_BYTES + 1).unwrap(),
            expected
        );
        assert_eq!(read.get(), 0);
    }
    assert_eq!(document.generation(), original_generation);
    assert_eq!(document.patches(), original_patches);
    if !logical.is_empty() {
        read.set(0);
        limit.set(MAX_READ_BYTES);
        document
            .overwrite(FileOffset::new(0), &[logical[0] ^ 0xff])
            .unwrap();
        assert!(
            scan.step(&mut document, 1).is_err(),
            "stale search accepted changed bytes"
        );
    }
}
