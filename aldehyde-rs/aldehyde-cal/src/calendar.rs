use polyepoxide_core::{oxide, Bond};

use crate::event::CalendarEvent;
use crate::freebusy::FreeBusy;
use crate::todo::CalendarTodo;

/// A calendar (collection of components)
#[oxide]
pub struct Calendar {
    pub name: String,
    pub description: Option<String>,
    pub events: Vec<Bond<CalendarEvent>>,
    pub todos: Vec<Bond<CalendarTodo>>,
    pub freebusy: Option<Bond<FreeBusy>>,
}
