use polyepoxide_core::oxide;

/// Participation status (PARTSTAT)
#[oxide]
pub enum ParticipationStatus {
    NeedsAction,
    Accepted,
    Declined,
    Tentative,
}

/// Role of attendee (ROLE)
#[oxide]
pub enum AttendeeRole {
    Chair,
    ReqParticipant,
    OptParticipant,
    NonParticipant,
}

/// Calendar user type (CUTYPE)
#[oxide]
pub enum CalendarUserType {
    Individual,
    Group,
    Resource,
    Room,
    Unknown,
}

/// An event attendee
#[oxide]
pub struct Attendee {
    pub email: String,
    pub common_name: Option<String>,
    pub status: ParticipationStatus,
    pub role: AttendeeRole,
    pub user_type: CalendarUserType,
    pub rsvp: bool,
}

/// Event organizer
#[oxide]
pub struct Organizer {
    pub email: String,
    pub common_name: Option<String>,
}
