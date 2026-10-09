//! `zempic.toml` discovery, merge and schema.

/// A calendar date, compared in calendar order without a date crate.
///
/// Fields are declared year, then month, then day, so the derived [`Ord`]
/// compares year first, then month, then day, which is calendar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDate {
    /// The full year, for example `2026`.
    pub year: u16,
    /// The month, `1` to `12`.
    pub month: u8,
    /// The day of the month, `1` to the month's length.
    pub day: u8,
}

impl CivilDate {
    /// The date today, read from the system clock.
    ///
    /// The clock is read against the Unix epoch and converted to a civil date;
    /// a clock before the epoch is treated as the epoch, since an ignore date
    /// that far in the past is expired regardless.
    pub fn today() -> Self {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        Self::from_days_since_epoch((seconds / 86_400) as i64)
    }

    /// The civil date `days` days after 1970-01-01.
    ///
    /// Uses Howard Hinnant's `civil_from_days`, which shifts the year to start
    /// in March so leap days fall at the end of the cycle and no month-length
    /// branch is needed.
    pub fn from_days_since_epoch(days: i64) -> Self {
        let shifted = days + 719_468;
        let era = shifted.div_euclid(146_097);
        let day_of_era = shifted.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let march_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * march_month + 2) / 5 + 1;
        let month = march_month + if march_month < 10 { 3 } else { -9 };
        let year = year_of_era + era * 400 + i64::from(month <= 2);

        CivilDate {
            year: year as u16,
            month: month as u8,
            day: day as u8,
        }
    }

    /// Parse a `YYYY-MM-DD` date, rejecting an impossible month or day.
    ///
    /// Returns [`None`] for anything but exactly four, two and two ASCII
    /// digits, a month outside `1` to `12`, a day outside the month's length,
    /// or a 29 February in a non-leap year.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();

        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }

        let year = digits(&bytes[0..4])? as u16;
        let month = digits(&bytes[5..7])? as u8;
        let day = digits(&bytes[8..10])? as u8;

        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }

        Some(CivilDate { year, month, day })
    }

    /// The date as a zero-padded `YYYY-MM-DD` string.
    pub fn as_iso8601(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Parse exactly `digits.len()` ASCII digits as a number, or [`None`].
fn digits(digits: &[u8]) -> Option<u32> {
    let mut value = 0u32;

    for byte in digits {
        if !byte.is_ascii_digit() {
            return None;
        }

        value = value * 10 + u32::from(byte - b'0');
    }

    Some(value)
}

/// Whether `year` is a leap year in the Gregorian calendar.
fn is_leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// The number of days in `month` of `year`.
fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::CivilDate;

    #[test]
    fn civil_date_parses_and_rejects_impossible_dates() {
        assert_eq!(
            CivilDate::parse("2026-12-31"),
            Some(CivilDate {
                year: 2026,
                month: 12,
                day: 31,
            })
        );
        assert_eq!(
            CivilDate::parse("2024-02-29"),
            Some(CivilDate {
                year: 2024,
                month: 2,
                day: 29,
            })
        );

        for text in [
            "2026-02-29",
            "2026-13-01",
            "2026-00-10",
            "2026-01-32",
            "2026-1-01",
            "26-01-01",
            "2026/01/01",
            "",
        ] {
            assert_eq!(CivilDate::parse(text), None, "`{text}` must be rejected");
        }
    }

    #[test]
    fn civil_date_converts_days_since_the_epoch() {
        assert_eq!(
            CivilDate::from_days_since_epoch(0),
            CivilDate {
                year: 1970,
                month: 1,
                day: 1,
            }
        );
        assert_eq!(
            CivilDate::from_days_since_epoch(11016),
            CivilDate {
                year: 2000,
                month: 2,
                day: 29,
            }
        );
        assert_eq!(
            CivilDate::from_days_since_epoch(19782),
            CivilDate {
                year: 2024,
                month: 2,
                day: 29,
            }
        );
        assert_eq!(
            CivilDate::from_days_since_epoch(20730),
            CivilDate {
                year: 2026,
                month: 10,
                day: 4,
            }
        );
        assert_eq!(
            CivilDate::from_days_since_epoch(20818),
            CivilDate {
                year: 2026,
                month: 12,
                day: 31,
            }
        );
    }

    #[test]
    fn civil_date_orders_chronologically() {
        assert!(CivilDate::parse("2026-01-31").unwrap() < CivilDate::parse("2026-02-01").unwrap());
        assert!(CivilDate::parse("1999-12-31").unwrap() < CivilDate::parse("2000-01-01").unwrap());
    }
}
