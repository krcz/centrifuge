use polyepoxide_core::{oxide, Bond};

use crate::alarm::Alarm;
use crate::time::DateTimeValue;

pub type TodoUid = String;

/// Todo status (STATUS)
#[oxide]
pub enum TodoStatus {
    NeedsAction,
    InProcess,
    Completed,
    Cancelled,
}

/// Calendar todo item (VTODO)
#[oxide]
pub struct CalendarTodo {
    pub uid: TodoUid,
    pub summary: String,
    pub description: Option<String>,
    pub priority: Option<u8>,
    pub percent_complete: Option<u8>,
    pub status: TodoStatus,
    pub due: Option<DateTimeValue>,
    pub completed: Option<i64>,
    pub alarms: Vec<Bond<Alarm>>,
    pub created: i64,
    pub last_modified: i64,
    pub sequence: u32,
}
