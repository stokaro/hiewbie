use std::fs::Metadata;
use std::time::UNIX_EPOCH;

use time::{Duration, OffsetDateTime};

pub(super) fn columns(metadata: &Metadata) -> (String, String) {
    timestamp(metadata)
        .and_then(|utc| dos_columns(display_clock(utc)))
        .unwrap_or_else(|| ("----------".into(), "--------".into()))
}

fn dos_columns(clock: OffsetDateTime) -> Option<(String, String)> {
    // The DOS conversion rounds to the next even second. HIEW shows the
    // encoded two-second counter as-is, including any minute or date carry.
    let rounded = clock.checked_add(Duration::SECOND)?;
    let ticks = rounded.second() / 2;
    Some((
        format!(
            "{:02}-{:02}-{:04}",
            rounded.day(),
            u8::from(rounded.month()),
            rounded.year()
        ),
        format!("{:02}:{:02}:{ticks:02}", rounded.hour(), rounded.minute()),
    ))
}

fn timestamp(metadata: &Metadata) -> Option<OffsetDateTime> {
    let nanos = match metadata.modified().ok()?.duration_since(UNIX_EPOCH) {
        Ok(age) => i128::try_from(age.as_nanos()).ok()?,
        Err(age) => -i128::try_from(age.duration().as_nanos()).ok()?,
    };
    OffsetDateTime::from_unix_timestamp_nanos(nanos).ok()
}

#[cfg(not(windows))]
fn display_clock(utc: OffsetDateTime) -> OffsetDateTime {
    utc
}

#[cfg(windows)]
fn display_clock(utc: OffsetDateTime) -> OffsetDateTime {
    windows_display_offset(utc)
        .and_then(|offset| utc.checked_to_offset(offset))
        .unwrap_or(utc)
}

#[cfg(windows)]
fn windows_display_offset(utc: OffsetDateTime) -> Option<time::UtcOffset> {
    use time::{Date, Month, UtcOffset};

    let offset_in = |month| {
        let date = Date::from_calendar_date(utc.year(), month, 15).ok()?;
        UtcOffset::local_offset_at(date.midnight().assume_utc()).ok()
    };
    // The recorded Windows build applied the standard offset and subtracted
    // the seasonal advance. Use opposite-season samples for the standard
    // offset and fall back to UTC if the host cannot provide its offset data.
    let standard = offset_in(Month::January)?.min(offset_in(Month::July)?);
    let actual = UtcOffset::local_offset_at(utc).ok()?;
    UtcOffset::from_whole_seconds(2 * standard.whole_seconds() - actual.whole_seconds()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dos_display_rounds_up_across_minutes_and_days() {
        for (seconds, expected_date, expected_time) in [
            (1_776_428_430, "17-04-2026", "12:20:15"),
            (1_776_428_439, "17-04-2026", "12:20:20"),
            (1_776_428_459, "17-04-2026", "12:21:00"),
            (1_767_225_599, "01-01-2026", "00:00:00"),
        ] {
            let date = OffsetDateTime::from_unix_timestamp(seconds).unwrap();
            let (date, clock) = dos_columns(date).unwrap();
            assert_eq!(
                (date.as_str(), clock.as_str()),
                (expected_date, expected_time)
            );
        }
    }
}
