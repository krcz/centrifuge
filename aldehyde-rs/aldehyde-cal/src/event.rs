use polyepoxide_core::{oxide, Bond};

use crate::alarm::Alarm;
use crate::attendee::{Attendee, Organizer};
use crate::recurrence::RecurrenceRule;
use crate::time::DateTimeValue;

pub type EventUid = String;

/// Calendar event (VEVENT)
#[oxide]
pub struct CalendarEvent {
    pub uid: EventUid,
    pub summary: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: DateTimeValue,
    pub end: Option<DateTimeValue>,
    pub recurrence_rule: Option<Bond<RecurrenceRule>>,
    pub recurrence_exceptions: Vec<i64>,
    pub organizer: Option<Bond<Organizer>>,
    pub attendees: Vec<Bond<Attendee>>,
    pub alarms: Vec<Bond<Alarm>>,
    pub created: i64,
    pub last_modified: i64,
    pub sequence: u32,
}
