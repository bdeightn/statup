//! When an incident's next update is due, as its team announces it.

use chrono::{DateTime, Duration, NaiveTime, Utc};

use crate::clock;

/// Longest delay a team may announce.
const MAX_DELAY_MINUTES: i64 = 24 * 60;

/// Up to this many hours, "3h" is a delay; from the next one, a time of day:
/// nobody promises news in nine hours, but "9h" is how French writes 09:00.
const MAX_DELAY_HOURS: i64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextUpdate {
    pub at: DateTime<Utc>,
    /// The delay it was given as, offered again with the following update.
    pub every_minutes: Option<i64>,
}

/// What the team wrote reads as neither a delay nor a time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable;

impl NextUpdate {
    /// Reads what the team wrote: a delay ("30", "30 min", "2 h") or a time
    /// of day ("14:30", "14h30", "1430", "9h"). `Ok(None)` when left blank.
    pub fn parse(text: &str, now: DateTime<Utc>) -> Result<Option<Self>, Unreadable> {
        let text = text.trim().to_lowercase();
        if text.is_empty() {
            return Ok(None);
        }
        let reading = read(&text).ok_or(Unreadable)?;
        let next = match reading {
            Reading::Delay(minutes) => Self {
                at: now + Duration::minutes(minutes),
                every_minutes: Some(minutes),
            },
            Reading::TimeOfDay(time) => Self {
                at: clock::next_occurrence(time, now).ok_or(Unreadable)?,
                every_minutes: None,
            },
        };
        Ok(Some(next))
    }
}

/// A remembered delay as the field shows it again: "30 min", "2 h".
pub fn delay_text(minutes: i64) -> String {
    if minutes % 60 == 0 && minutes / 60 <= MAX_DELAY_HOURS {
        format!("{} h", minutes / 60)
    } else {
        format!("{minutes} min")
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Reading {
    Delay(i64),
    TimeOfDay(NaiveTime),
}

fn read(text: &str) -> Option<Reading> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some((hours, minutes)) = compact.split_once([':', 'h'])
        && !minutes.is_empty()
    {
        return time_of_day(hours, minutes);
    }
    if let Some(hours) = compact.strip_suffix('h') {
        let hours: i64 = digits(hours, 2)?;
        return if hours <= MAX_DELAY_HOURS {
            delay(hours * 60)
        } else {
            time_of_day(&hours.to_string(), "00")
        };
    }
    let number = ["min", "mn", "m"]
        .iter()
        .find_map(|unit| compact.strip_suffix(unit))
        .unwrap_or(&compact);
    if number.len() == 4 {
        return time_of_day(&number[..2], &number[2..]);
    }
    delay(digits(number, 3)?)
}

fn time_of_day(hours: &str, minutes: &str) -> Option<Reading> {
    if minutes.len() != 2 {
        return None;
    }
    let time = NaiveTime::from_hms_opt(
        u32::try_from(digits(hours, 2)?).ok()?,
        u32::try_from(digits(minutes, 2)?).ok()?,
        0,
    )?;
    Some(Reading::TimeOfDay(time))
}

fn delay(minutes: i64) -> Option<Reading> {
    (1..=MAX_DELAY_MINUTES)
        .contains(&minutes)
        .then_some(Reading::Delay(minutes))
}

/// A number of at most `width` digits, nothing else.
fn digits(text: &str, width: usize) -> Option<i64> {
    if text.is_empty() || text.len() > width || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hours: u32, minutes: u32) -> Option<Reading> {
        NaiveTime::from_hms_opt(hours, minutes, 0).map(Reading::TimeOfDay)
    }

    #[test]
    fn delays_read_in_minutes_or_hours() {
        assert_eq!(read("30"), Some(Reading::Delay(30)));
        assert_eq!(read("30 min"), Some(Reading::Delay(30)));
        assert_eq!(read("45mn"), Some(Reading::Delay(45)));
        assert_eq!(read("90m"), Some(Reading::Delay(90)));
        assert_eq!(read("1 h"), Some(Reading::Delay(60)));
        assert_eq!(read("4h"), Some(Reading::Delay(240)));
    }

    #[test]
    fn times_of_day_read_with_their_minutes() {
        assert_eq!(read("14:30"), at(14, 30));
        assert_eq!(read("14h30"), at(14, 30));
        assert_eq!(read("9:05"), at(9, 5));
        assert_eq!(read("1430"), at(14, 30));
        assert_eq!(read("14 h 00"), at(14, 0));
    }

    #[test]
    fn hours_past_a_short_delay_are_a_time_of_day() {
        assert_eq!(read("9h"), at(9, 0));
        assert_eq!(read("15 h"), at(15, 0));
    }

    #[test]
    fn nonsense_and_out_of_range_are_refused() {
        for text in [
            "", "soon", "25:00", "14:3", "14:300", "0", "2500", "-5", "1,5 h", "99h",
        ] {
            assert_eq!(read(text), None, "{text}");
        }
    }

    #[test]
    fn a_delay_is_remembered_and_a_time_is_not() {
        let now = Utc::now();
        let delay = NextUpdate::parse("30 min", now).unwrap().unwrap();
        assert_eq!(delay.at, now + Duration::minutes(30));
        assert_eq!(delay.every_minutes, Some(30));
        let time = NextUpdate::parse("14:30", now).unwrap().unwrap();
        assert_eq!(time.every_minutes, None);
        assert!(time.at > now && time.at <= now + Duration::days(1));
    }

    #[test]
    fn blank_is_no_announcement() {
        assert_eq!(NextUpdate::parse("  ", Utc::now()), Ok(None));
        assert_eq!(NextUpdate::parse("later", Utc::now()), Err(Unreadable));
    }

    #[test]
    fn a_remembered_delay_reads_back_the_way_it_parses() {
        for minutes in [5, 30, 60, 90, 240, 300] {
            let text = delay_text(minutes);
            assert_eq!(read(&text), Some(Reading::Delay(minutes)), "{text}");
        }
    }
}
