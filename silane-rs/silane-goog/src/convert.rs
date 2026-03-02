use aldehyde_cal::{
    Alarm, AlarmAction, AlarmTrigger, Attendee, AttendeeRole, CalendarEvent, CalendarTodo,
    CalendarUserType, DateTime, DateTimeValue, DateValue, Duration, Organizer, ParticipationStatus,
    RecurrenceRule, TodoStatus,
};
use chrono::Datelike;
use google_calendar3::api::{Event, EventAttendee, EventDateTime, EventReminder};
use google_tasks1::api::Task;
use polyepoxide_core::{Bond, Solvent};

use crate::error::GoogleError;
use crate::rrule::parse_rrule;

pub fn convert_event(event: &Event, solvent: &mut Solvent) -> Result<CalendarEvent, GoogleError> {
    let uid = event
        .id
        .clone()
        .ok_or_else(|| GoogleError::Parse("event missing id".to_string()))?;

    let summary = event.summary.clone().unwrap_or_default();

    let start = event
        .start
        .as_ref()
        .ok_or_else(|| GoogleError::Parse("event missing start".to_string()))?;
    let start = convert_datetime(start)?;

    let end = event.end.as_ref().map(convert_datetime).transpose()?;

    let recurrence_rule = convert_recurrence(&event.recurrence, solvent)?;
    let recurrence_exceptions = convert_recurrence_exceptions(event);

    let organizer = event.organizer.as_ref().map(|o| {
        solvent.bond(Organizer {
            email: o.email.clone().unwrap_or_default(),
            common_name: o.display_name.clone(),
        })
    });

    let attendees = event
        .attendees
        .as_ref()
        .map(|list| {
            list.iter()
                .map(|a| solvent.bond(convert_attendee(a)))
                .collect()
        })
        .unwrap_or_default();

    let alarms = convert_reminders(event, solvent);

    let created = event.created.map(|dt| dt.timestamp()).unwrap_or(0);
    let last_modified = event.updated.map(|dt| dt.timestamp()).unwrap_or(0);
    let sequence = event.sequence.unwrap_or(0) as u32;

    Ok(CalendarEvent {
        uid,
        summary,
        description: event.description.clone(),
        location: event.location.clone(),
        start,
        end,
        recurrence_rule,
        recurrence_exceptions,
        organizer,
        attendees,
        alarms,
        created,
        last_modified,
        sequence,
    })
}

fn convert_datetime(edt: &EventDateTime) -> Result<DateTimeValue, GoogleError> {
    if let Some(ref date) = edt.date {
        // All-day event: date only (NaiveDate)
        Ok(DateTimeValue::Date(DateValue {
            year: date.year(),
            month: date.month() as u8,
            day: date.day() as u8,
        }))
    } else if let Some(ref dt) = edt.date_time {
        // Timed event (DateTime<Utc>)
        let timezone = edt.time_zone.clone().unwrap_or_else(|| "UTC".to_string());
        Ok(DateTimeValue::DateTime(DateTime {
            utc_timestamp: dt.timestamp(),
            timezone,
        }))
    } else {
        Err(GoogleError::Parse(
            "EventDateTime has neither date nor dateTime".to_string(),
        ))
    }
}

fn convert_recurrence(
    recurrence: &Option<Vec<String>>,
    solvent: &mut Solvent,
) -> Result<Option<Bond<RecurrenceRule>>, GoogleError> {
    let Some(lines) = recurrence else {
        return Ok(None);
    };

    // Find the RRULE line (ignore EXDATE etc for now)
    for line in lines {
        if line.starts_with("RRULE:") || line.starts_with("FREQ=") {
            let rule = parse_rrule(line)?;
            return Ok(Some(solvent.bond(rule)));
        }
    }

    Ok(None)
}

fn convert_recurrence_exceptions(event: &Event) -> Vec<i64> {
    // Google Calendar returns recurrence exceptions in the recurrence array as EXDATE lines
    let Some(ref recurrence) = event.recurrence else {
        return vec![];
    };

    let mut exceptions = Vec::new();
    for line in recurrence {
        if let Some(exdate) = line.strip_prefix("EXDATE;") {
            // Format: EXDATE;TZID=Etc/UTC:20240101T100000
            if let Some((_params, value)) = exdate.split_once(':') {
                for date_str in value.split(',') {
                    if let Ok(ts) = parse_exdate(date_str) {
                        exceptions.push(ts);
                    }
                }
            }
        } else if let Some(exdate) = line.strip_prefix("EXDATE:") {
            for date_str in exdate.split(',') {
                if let Ok(ts) = parse_exdate(date_str) {
                    exceptions.push(ts);
                }
            }
        }
    }
    exceptions
}

fn parse_exdate(s: &str) -> Result<i64, GoogleError> {
    // Try datetime format first
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S") {
        return Ok(dt.and_utc().timestamp());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%SZ") {
        return Ok(dt.and_utc().timestamp());
    }
    // Try date-only format
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y%m%d") {
        return Ok(date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp());
    }
    Err(GoogleError::Parse(format!("invalid EXDATE: {}", s)))
}

fn convert_attendee(attendee: &EventAttendee) -> Attendee {
    let status = match attendee.response_status.as_deref() {
        Some("needsAction") => ParticipationStatus::NeedsAction,
        Some("accepted") => ParticipationStatus::Accepted,
        Some("declined") => ParticipationStatus::Declined,
        Some("tentative") => ParticipationStatus::Tentative,
        _ => ParticipationStatus::NeedsAction,
    };

    let role = if attendee.optional.unwrap_or(false) {
        AttendeeRole::OptParticipant
    } else {
        AttendeeRole::ReqParticipant
    };

    let user_type = if attendee.resource.unwrap_or(false) {
        CalendarUserType::Resource
    } else {
        CalendarUserType::Individual
    };

    Attendee {
        email: attendee.email.clone().unwrap_or_default(),
        common_name: attendee.display_name.clone(),
        status,
        role,
        user_type,
        rsvp: false,
    }
}

fn convert_reminders(event: &Event, solvent: &mut Solvent) -> Vec<Bond<Alarm>> {
    let Some(ref reminders) = event.reminders else {
        return vec![];
    };

    let Some(ref overrides) = reminders.overrides else {
        return vec![];
    };

    overrides
        .iter()
        .filter_map(|r| convert_reminder(r, solvent))
        .collect()
}

fn convert_reminder(reminder: &EventReminder, solvent: &mut Solvent) -> Option<Bond<Alarm>> {
    let minutes = reminder.minutes?;
    let action = match reminder.method.as_deref() {
        Some("email") => AlarmAction::Email,
        Some("popup") | Some("display") => AlarmAction::Display,
        _ => AlarmAction::Display,
    };

    let alarm = Alarm {
        action,
        trigger: AlarmTrigger::BeforeStart(Duration {
            seconds: minutes as i64 * 60,
            negative: false,
        }),
        description: None,
        repeat_count: None,
        repeat_duration: None,
    };

    Some(solvent.bond(alarm))
}

pub fn convert_task(task: &Task) -> Result<CalendarTodo, GoogleError> {
    let uid = task
        .id
        .clone()
        .ok_or_else(|| GoogleError::Parse("task missing id".to_string()))?;

    let summary = task.title.clone().unwrap_or_default();

    let status = match task.status.as_deref() {
        Some("completed") => TodoStatus::Completed,
        Some("needsAction") | _ => TodoStatus::NeedsAction,
    };

    // Tasks API returns dates as RFC3339 strings
    let due = task
        .due
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| {
            DateTimeValue::DateTime(DateTime {
                utc_timestamp: dt.timestamp(),
                timezone: "UTC".to_string(),
            })
        });

    let completed = task
        .completed
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp());

    let created = task
        .updated
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    Ok(CalendarTodo {
        uid,
        summary,
        description: task.notes.clone(),
        priority: None,
        percent_complete: None,
        status,
        due,
        completed,
        alarms: vec![],
        created,
        last_modified: created,
        sequence: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_date_only() {
        let date = chrono::NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let edt = EventDateTime {
            date: Some(date),
            date_time: None,
            time_zone: None,
        };
        let result = convert_datetime(&edt).unwrap();
        match result {
            DateTimeValue::Date(d) => {
                assert_eq!(d.year, 2024);
                assert_eq!(d.month, 6);
                assert_eq!(d.day, 15);
            }
            _ => panic!("expected Date"),
        }
    }

    #[test]
    fn convert_datetime_with_tz() {
        let dt = chrono::DateTime::parse_from_rfc3339("2024-06-15T10:30:00+02:00")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let edt = EventDateTime {
            date: None,
            date_time: Some(dt),
            time_zone: Some("Europe/Warsaw".to_string()),
        };
        let result = convert_datetime(&edt).unwrap();
        match result {
            DateTimeValue::DateTime(dt) => {
                assert_eq!(dt.timezone, "Europe/Warsaw");
            }
            _ => panic!("expected DateTime"),
        }
    }

    #[test]
    fn convert_attendee_accepted() {
        let ea = EventAttendee {
            email: Some("test@example.com".to_string()),
            display_name: Some("Test User".to_string()),
            response_status: Some("accepted".to_string()),
            optional: Some(false),
            resource: Some(false),
            ..Default::default()
        };
        let attendee = convert_attendee(&ea);
        assert_eq!(attendee.email, "test@example.com");
        assert!(matches!(attendee.status, ParticipationStatus::Accepted));
        assert!(matches!(attendee.role, AttendeeRole::ReqParticipant));
    }
}
