use chrono::{Datelike, NaiveDate, Utc};
use chrono_tz::Asia::Manila;

use crate::error::{PcsoError, Result};

pub const DATE_FMT: &str = "%m-%d-%Y";

/// A user-supplied date specifier. Either a single day (`MM-dd-yyyy`) or a
/// whole month (`MonthName-yyyy`).
///
/// A whole-month spec resolves to the first day of the month when used as a
/// `--from`, and to the last day of the month when used as a `--to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateSpec {
    Day(NaiveDate),
    Month { year: i32, month: u32 },
}

impl DateSpec {
    /// Convert into the first-day (range-start) representation.
    pub fn first_day(self) -> NaiveDate {
        match self {
            DateSpec::Day(d) => d,
            DateSpec::Month { year, month } => NaiveDate::from_ymd_opt(year, month, 1)
                .expect("validated month/year"),
        }
    }

    /// Convert into the last-day (range-end) representation.
    pub fn last_day(self) -> NaiveDate {
        match self {
            DateSpec::Day(d) => d,
            DateSpec::Month { year, month } => last_day_of_month(year, month),
        }
    }
}

pub fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, DATE_FMT)
        .map_err(|e| PcsoError::InvalidDate(s.to_string(), e.to_string()))
}

/// Parse either `MM-dd-yyyy` (single day) or `MonthName-yyyy` (whole month).
/// Month names are case-insensitive and accept full names ("March") or the
/// standard 3-letter abbreviations ("Mar").
pub fn parse_date_spec(s: &str) -> Result<DateSpec> {
    if let Some(spec) = try_parse_month_spec(s) {
        return Ok(spec);
    }
    parse_date(s).map(DateSpec::Day)
}

pub fn parse_date_spec_clap(s: &str) -> std::result::Result<DateSpec, String> {
    parse_date_spec(s).map_err(|e| e.to_string())
}

fn try_parse_month_spec(s: &str) -> Option<DateSpec> {
    // Expect exactly one `-` separating "Month" and "yyyy".
    let (m_str, y_str) = s.rsplit_once('-')?;
    if m_str.is_empty() {
        return None;
    }
    let year: i32 = y_str.parse().ok()?;
    let month = parse_month_name(m_str)?;
    // Sanity: year/month must form a valid date.
    NaiveDate::from_ymd_opt(year, month, 1)?;
    Some(DateSpec::Month { year, month })
}

fn parse_month_name(s: &str) -> Option<u32> {
    match s.trim().to_ascii_lowercase().as_str() {
        "january"   | "jan" => Some(1),
        "february"  | "feb" => Some(2),
        "march"     | "mar" => Some(3),
        "april"     | "apr" => Some(4),
        "may"               => Some(5),
        "june"      | "jun" => Some(6),
        "july"      | "jul" => Some(7),
        "august"    | "aug" => Some(8),
        "september" | "sep" => Some(9),
        "october"   | "oct" => Some(10),
        "november"  | "nov" => Some(11),
        "december"  | "dec" => Some(12),
        _ => None,
    }
}

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    if month == 12 {
        NaiveDate::from_ymd_opt(year, 12, 31).expect("dec 31 always valid")
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
            .expect("validated month")
            .pred_opt()
            .expect("dec 31 of previous year always valid")
    }
}

pub fn today_manila() -> NaiveDate {
    Utc::now().with_timezone(&Manila).date_naive()
}

pub fn format_for_path(d: NaiveDate) -> String {
    d.format(DATE_FMT).to_string()
}

pub fn month_name(d: NaiveDate) -> &'static str {
    match d.month() {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => unreachable!("chrono guarantees 1..=12"),
    }
}

pub fn range_inclusive(from: NaiveDate, to: NaiveDate) -> Result<Vec<NaiveDate>> {
    if from > to {
        return Err(PcsoError::InvalidRange {
            from: format_for_path(from),
            to: format_for_path(to),
        });
    }
    let mut out = Vec::new();
    let mut d = from;
    while d <= to {
        out.push(d);
        d = d.succ_opt().expect("date overflow");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mmddyyyy() {
        let d = parse_date("03-05-2026").unwrap();
        assert_eq!(d, NaiveDate::from_ymd_opt(2026, 3, 5).unwrap());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_date("2026-03-05").is_err());
        assert!(parse_date("13-01-2026").is_err());
    }

    #[test]
    fn range_inclusive_works() {
        let from = parse_date("03-01-2026").unwrap();
        let to = parse_date("03-03-2026").unwrap();
        assert_eq!(range_inclusive(from, to).unwrap().len(), 3);
    }

    #[test]
    fn range_rejects_reversed() {
        let from = parse_date("03-05-2026").unwrap();
        let to = parse_date("03-01-2026").unwrap();
        assert!(range_inclusive(from, to).is_err());
    }

    #[test]
    fn month_name_full() {
        assert_eq!(month_name(parse_date("03-05-2026").unwrap()), "March");
    }

    #[test]
    fn parses_day_spec() {
        let s = parse_date_spec("03-05-2026").unwrap();
        assert_eq!(s, DateSpec::Day(NaiveDate::from_ymd_opt(2026, 3, 5).unwrap()));
        assert_eq!(s.first_day(), s.last_day());
    }

    #[test]
    fn parses_month_spec_full_name() {
        let s = parse_date_spec("March-2022").unwrap();
        assert_eq!(s, DateSpec::Month { year: 2022, month: 3 });
        assert_eq!(s.first_day(), NaiveDate::from_ymd_opt(2022, 3, 1).unwrap());
        assert_eq!(s.last_day(),  NaiveDate::from_ymd_opt(2022, 3, 31).unwrap());
    }

    #[test]
    fn parses_month_spec_abbrev_case_insensitive() {
        assert_eq!(
            parse_date_spec("feb-2024").unwrap(),
            DateSpec::Month { year: 2024, month: 2 }
        );
        // leap year — last day is the 29th
        assert_eq!(
            parse_date_spec("feb-2024").unwrap().last_day(),
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()
        );
    }

    #[test]
    fn parses_december_edge() {
        let s = parse_date_spec("December-2025").unwrap();
        assert_eq!(s.first_day(), NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());
        assert_eq!(s.last_day(),  NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
    }

    #[test]
    fn rejects_garbage_spec() {
        assert!(parse_date_spec("13-01-2026").is_err());
        assert!(parse_date_spec("Smarch-2024").is_err());
        assert!(parse_date_spec("2024-March").is_err()); // wrong order
        assert!(parse_date_spec("March").is_err());
    }
}
