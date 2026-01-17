use polyepoxide_core::oxide;

/// IANA timezone identifier (e.g., "Europe/Warsaw", "America/New_York")
pub type TimezoneId = String;

/// Date-only value for all-day events
#[oxide]
pub struct DateValue {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// UTC timestamp with associated timezone for display
#[oxide]
pub struct DateTime {
    pub utc_timestamp: i64,
    pub timezone: TimezoneId,
}

/// Duration in seconds
#[oxide]
pub struct Duration {
    pub seconds: i64,
    pub negative: bool,
}

/// Either a date (all-day) or datetime (timed)
#[oxide]
pub enum DateTimeValue {
    Date(DateValue),
    DateTime(DateTime),
}
