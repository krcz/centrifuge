use aldehyde_cal::{
    Alarm, AlarmAction, AlarmTrigger, Attendee, AttendeeRole, Calendar, CalendarEvent,
    CalendarTodo, CalendarUserType, DateTime, DateTimeValue, DateValue, Duration, Frequency,
    Organizer, ParticipationStatus, RecurrenceRule, TodoStatus, Weekday,
};
use polyepoxide_core::{Bond, Oxide, Solvent};

#[test]
fn create_simple_event() {
    let mut solvent = Solvent::new();

    let event = CalendarEvent {
        uid: "event-001".to_string(),
        summary: "Team Meeting".to_string(),
        description: Some("Weekly sync".to_string()),
        location: Some("Conference Room A".to_string()),
        start: DateTimeValue::DateTime(DateTime {
            utc_timestamp: 1704110400,
            timezone: "Europe/Warsaw".to_string(),
        }),
        end: Some(DateTimeValue::DateTime(DateTime {
            utc_timestamp: 1704114000,
            timezone: "Europe/Warsaw".to_string(),
        })),
        recurrence_rule: None,
        recurrence_exceptions: vec![],
        organizer: None,
        attendees: vec![],
        alarms: vec![],
        created: 1704067200,
        last_modified: 1704067200,
        sequence: 0,
    };

    let cell = solvent.add(event);
    assert_eq!(cell.value().summary, "Team Meeting");
}

#[test]
fn create_all_day_event() {
    let mut solvent = Solvent::new();

    let event = CalendarEvent {
        uid: "allday-001".to_string(),
        summary: "Company Holiday".to_string(),
        description: None,
        location: None,
        start: DateTimeValue::Date(DateValue {
            year: 2024,
            month: 12,
            day: 25,
        }),
        end: Some(DateTimeValue::Date(DateValue {
            year: 2024,
            month: 12,
            day: 26,
        })),
        recurrence_rule: None,
        recurrence_exceptions: vec![],
        organizer: None,
        attendees: vec![],
        alarms: vec![],
        created: 1704067200,
        last_modified: 1704067200,
        sequence: 0,
    };

    let cell = solvent.add(event);
    match &cell.value().start {
        DateTimeValue::Date(d) => {
            assert_eq!(d.year, 2024);
            assert_eq!(d.month, 12);
            assert_eq!(d.day, 25);
        }
        _ => panic!("Expected Date variant"),
    }
}

#[test]
fn create_recurring_event_with_attendees() {
    let mut solvent = Solvent::new();

    let organizer = Organizer {
        email: "alice@example.com".to_string(),
        common_name: Some("Alice".to_string()),
    };
    let organizer_cell = solvent.add(organizer);

    let attendee = Attendee {
        email: "bob@example.com".to_string(),
        common_name: Some("Bob".to_string()),
        status: ParticipationStatus::NeedsAction,
        role: AttendeeRole::ReqParticipant,
        user_type: CalendarUserType::Individual,
        rsvp: true,
    };
    let attendee_cell = solvent.add(attendee);

    let rule = RecurrenceRule {
        frequency: Frequency::Weekly,
        interval: 1,
        count: Some(10),
        until: None,
        by_day: vec![Weekday::Monday, Weekday::Wednesday],
        by_month_day: vec![],
        by_month: vec![],
    };
    let rule_cell = solvent.add(rule);

    let alarm = Alarm {
        action: AlarmAction::Display,
        trigger: AlarmTrigger::BeforeStart(Duration {
            seconds: 900,
            negative: false,
        }),
        description: Some("Meeting in 15 minutes".to_string()),
        repeat_count: None,
        repeat_duration: None,
    };
    let alarm_cell = solvent.add(alarm);

    let event = CalendarEvent {
        uid: "recurring-001".to_string(),
        summary: "Standup".to_string(),
        description: None,
        location: None,
        start: DateTimeValue::DateTime(DateTime {
            utc_timestamp: 1704096000,
            timezone: "Europe/Warsaw".to_string(),
        }),
        end: None,
        recurrence_rule: Some(Bond::from_cell(rule_cell)),
        recurrence_exceptions: vec![],
        organizer: Some(Bond::from_cell(organizer_cell)),
        attendees: vec![Bond::from_cell(attendee_cell)],
        alarms: vec![Bond::from_cell(alarm_cell)],
        created: 1704067200,
        last_modified: 1704067200,
        sequence: 0,
    };

    let event_cell = solvent.add(event);
    assert!(event_cell.value().recurrence_rule.is_some());
    assert_eq!(event_cell.value().attendees.len(), 1);
    assert_eq!(event_cell.value().alarms.len(), 1);
}

#[test]
fn create_todo_with_progress() {
    let mut solvent = Solvent::new();

    let alarm = Alarm {
        action: AlarmAction::Display,
        trigger: AlarmTrigger::BeforeStart(Duration {
            seconds: 3600,
            negative: false,
        }),
        description: Some("Task due soon".to_string()),
        repeat_count: None,
        repeat_duration: None,
    };
    let alarm_cell = solvent.add(alarm);

    let todo = CalendarTodo {
        uid: "todo-001".to_string(),
        summary: "Review PR".to_string(),
        description: Some("Review the calendar implementation PR".to_string()),
        priority: Some(2),
        percent_complete: Some(50),
        status: TodoStatus::InProcess,
        due: Some(DateTimeValue::DateTime(DateTime {
            utc_timestamp: 1704153600,
            timezone: "Europe/Warsaw".to_string(),
        })),
        completed: None,
        alarms: vec![Bond::from_cell(alarm_cell)],
        created: 1704067200,
        last_modified: 1704067200,
        sequence: 0,
    };

    let todo_cell = solvent.add(todo);
    assert_eq!(todo_cell.value().priority, Some(2));
    assert_eq!(todo_cell.value().percent_complete, Some(50));
    match todo_cell.value().status {
        TodoStatus::InProcess => {}
        _ => panic!("Expected InProcess status"),
    }
}

#[test]
fn full_calendar() {
    let mut solvent = Solvent::new();

    let event = CalendarEvent {
        uid: "event-001".to_string(),
        summary: "Meeting".to_string(),
        description: None,
        location: None,
        start: DateTimeValue::DateTime(DateTime {
            utc_timestamp: 1704110400,
            timezone: "UTC".to_string(),
        }),
        end: None,
        recurrence_rule: None,
        recurrence_exceptions: vec![],
        organizer: None,
        attendees: vec![],
        alarms: vec![],
        created: 1704067200,
        last_modified: 1704067200,
        sequence: 0,
    };
    let event_cell = solvent.add(event);

    let todo = CalendarTodo {
        uid: "todo-001".to_string(),
        summary: "Task".to_string(),
        description: None,
        priority: None,
        percent_complete: None,
        status: TodoStatus::NeedsAction,
        due: None,
        completed: None,
        alarms: vec![],
        created: 1704067200,
        last_modified: 1704067200,
        sequence: 0,
    };
    let todo_cell = solvent.add(todo);

    let calendar = Calendar {
        name: "Personal".to_string(),
        description: Some("My personal calendar".to_string()),
        events: vec![Bond::from_cell(event_cell)],
        todos: vec![Bond::from_cell(todo_cell)],
        freebusy: None,
    };

    let calendar_cell = solvent.add(calendar);
    assert_eq!(calendar_cell.value().name, "Personal");
    assert_eq!(calendar_cell.value().events.len(), 1);
    assert_eq!(calendar_cell.value().todos.len(), 1);
}

#[test]
fn serialization_roundtrip() {
    let event = CalendarEvent {
        uid: "test-event".to_string(),
        summary: "Test".to_string(),
        description: None,
        location: None,
        start: DateTimeValue::DateTime(DateTime {
            utc_timestamp: 1704110400,
            timezone: "UTC".to_string(),
        }),
        end: None,
        recurrence_rule: None,
        recurrence_exceptions: vec![],
        organizer: None,
        attendees: vec![],
        alarms: vec![],
        created: 1704067200,
        last_modified: 1704067200,
        sequence: 0,
    };

    let bytes = event.to_bytes();
    let restored = CalendarEvent::from_bytes(&bytes).unwrap();

    assert_eq!(restored.uid, event.uid);
    assert_eq!(restored.summary, event.summary);
}
