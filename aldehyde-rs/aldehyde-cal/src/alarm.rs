use polyepoxide_core::oxide;

use crate::time::Duration;

/// Alarm action type
#[oxide]
pub enum AlarmAction {
    Display,
    Audio,
    Email,
}

/// Alarm trigger - when the alarm fires relative to event
#[oxide]
pub enum AlarmTrigger {
    BeforeStart(Duration),
    AfterEnd(Duration),
    Absolute(i64),
}

/// A calendar alarm (VALARM)
#[oxide]
pub struct Alarm {
    pub action: AlarmAction,
    pub trigger: AlarmTrigger,
    pub description: Option<String>,
    pub repeat_count: Option<u32>,
    pub repeat_duration: Option<Duration>,
}
