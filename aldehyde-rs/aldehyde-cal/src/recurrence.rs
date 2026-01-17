use polyepoxide_core::oxide;

/// Recurrence frequency
#[oxide]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Day of week for weekly recurrence
#[oxide]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

/// Basic recurrence rule (RRULE)
#[oxide]
pub struct RecurrenceRule {
    pub frequency: Frequency,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<i64>,
    pub by_day: Vec<Weekday>,
    pub by_month_day: Vec<i8>,
    pub by_month: Vec<u8>,
}
