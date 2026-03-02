use aldehyde_cal::{Frequency, RecurrenceRule, Weekday};

use crate::error::GoogleError;

/// Parse an RRULE string into a RecurrenceRule.
/// Handles formats like:
/// - `RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,FR`
/// - `FREQ=DAILY;COUNT=10`
pub fn parse_rrule(rrule: &str) -> Result<RecurrenceRule, GoogleError> {
    let rule_part = rrule.strip_prefix("RRULE:").unwrap_or(rrule);

    let mut frequency = None;
    let mut interval = 1;
    let mut count = None;
    let mut until = None;
    let mut by_day = Vec::new();
    let mut by_month_day = Vec::new();
    let mut by_month = Vec::new();

    for part in rule_part.split(';') {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| GoogleError::Parse(format!("invalid RRULE part: {}", part)))?;

        match key {
            "FREQ" => {
                frequency = Some(parse_frequency(value)?);
            }
            "INTERVAL" => {
                interval = value
                    .parse()
                    .map_err(|_| GoogleError::Parse(format!("invalid INTERVAL: {}", value)))?;
            }
            "COUNT" => {
                count = Some(
                    value
                        .parse()
                        .map_err(|_| GoogleError::Parse(format!("invalid COUNT: {}", value)))?,
                );
            }
            "UNTIL" => {
                until = Some(parse_until(value)?);
            }
            "BYDAY" => {
                by_day = parse_by_day(value)?;
            }
            "BYMONTHDAY" => {
                by_month_day = parse_by_month_day(value)?;
            }
            "BYMONTH" => {
                by_month = parse_by_month(value)?;
            }
            _ => {
                // Ignore unknown parts
            }
        }
    }

    let frequency =
        frequency.ok_or_else(|| GoogleError::Parse("RRULE missing FREQ".to_string()))?;

    Ok(RecurrenceRule {
        frequency,
        interval,
        count,
        until,
        by_day,
        by_month_day,
        by_month,
    })
}

fn parse_frequency(s: &str) -> Result<Frequency, GoogleError> {
    match s {
        "DAILY" => Ok(Frequency::Daily),
        "WEEKLY" => Ok(Frequency::Weekly),
        "MONTHLY" => Ok(Frequency::Monthly),
        "YEARLY" => Ok(Frequency::Yearly),
        _ => Err(GoogleError::Parse(format!("unknown FREQ: {}", s))),
    }
}

fn parse_until(s: &str) -> Result<i64, GoogleError> {
    // UNTIL can be either a DATE (YYYYMMDD) or DATETIME (YYYYMMDDTHHMMSSZ)
    let dt = if s.contains('T') {
        chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%SZ")
            .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S"))
            .map_err(|e| GoogleError::Parse(format!("invalid UNTIL datetime {}: {}", s, e)))?
    } else {
        let date = chrono::NaiveDate::parse_from_str(s, "%Y%m%d")
            .map_err(|e| GoogleError::Parse(format!("invalid UNTIL date {}: {}", s, e)))?;
        date.and_hms_opt(0, 0, 0).unwrap()
    };
    Ok(dt.and_utc().timestamp())
}

fn parse_by_day(s: &str) -> Result<Vec<Weekday>, GoogleError> {
    s.split(',')
        .map(|day| {
            // Handle both "MO" and "+1MO" (nth weekday) formats
            // For now, ignore the ordinal prefix
            let day_str =
                day.trim_start_matches(|c: char| c.is_ascii_digit() || c == '+' || c == '-');
            parse_weekday(day_str)
        })
        .collect()
}

fn parse_weekday(s: &str) -> Result<Weekday, GoogleError> {
    match s {
        "MO" => Ok(Weekday::Monday),
        "TU" => Ok(Weekday::Tuesday),
        "WE" => Ok(Weekday::Wednesday),
        "TH" => Ok(Weekday::Thursday),
        "FR" => Ok(Weekday::Friday),
        "SA" => Ok(Weekday::Saturday),
        "SU" => Ok(Weekday::Sunday),
        _ => Err(GoogleError::Parse(format!("unknown weekday: {}", s))),
    }
}

fn parse_by_month_day(s: &str) -> Result<Vec<i8>, GoogleError> {
    s.split(',')
        .map(|d| {
            d.parse()
                .map_err(|_| GoogleError::Parse(format!("invalid BYMONTHDAY: {}", d)))
        })
        .collect()
}

fn parse_by_month(s: &str) -> Result<Vec<u8>, GoogleError> {
    s.split(',')
        .map(|m| {
            m.parse()
                .map_err(|_| GoogleError::Parse(format!("invalid BYMONTH: {}", m)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_daily() {
        let rule = parse_rrule("RRULE:FREQ=DAILY").unwrap();
        assert!(matches!(rule.frequency, Frequency::Daily));
        assert_eq!(rule.interval, 1);
        assert_eq!(rule.count, None);
    }

    #[test]
    fn parse_weekly_with_interval() {
        let rule = parse_rrule("FREQ=WEEKLY;INTERVAL=2").unwrap();
        assert!(matches!(rule.frequency, Frequency::Weekly));
        assert_eq!(rule.interval, 2);
    }

    #[test]
    fn parse_weekly_with_days() {
        let rule = parse_rrule("RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR").unwrap();
        assert!(matches!(rule.frequency, Frequency::Weekly));
        assert_eq!(rule.by_day.len(), 3);
        assert!(matches!(rule.by_day[0], Weekday::Monday));
        assert!(matches!(rule.by_day[1], Weekday::Wednesday));
        assert!(matches!(rule.by_day[2], Weekday::Friday));
    }

    #[test]
    fn parse_monthly_with_count() {
        let rule = parse_rrule("FREQ=MONTHLY;BYMONTHDAY=15;COUNT=10").unwrap();
        assert!(matches!(rule.frequency, Frequency::Monthly));
        assert_eq!(rule.by_month_day, vec![15]);
        assert_eq!(rule.count, Some(10));
    }

    #[test]
    fn parse_yearly_with_until() {
        let rule = parse_rrule("FREQ=YEARLY;BYMONTH=6;UNTIL=20251231T235959Z").unwrap();
        assert!(matches!(rule.frequency, Frequency::Yearly));
        assert_eq!(rule.by_month, vec![6]);
        assert!(rule.until.is_some());
    }

    #[test]
    fn parse_with_ordinal_byday() {
        // First Monday of month
        let rule = parse_rrule("FREQ=MONTHLY;BYDAY=+1MO").unwrap();
        assert!(matches!(rule.frequency, Frequency::Monthly));
        assert_eq!(rule.by_day.len(), 1);
        assert!(matches!(rule.by_day[0], Weekday::Monday));
    }
}
