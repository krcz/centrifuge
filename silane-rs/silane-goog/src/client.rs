use aldehyde_cal::{Calendar, CalendarEvent, CalendarTodo};
use google_calendar3::CalendarHub;
use google_tasks1::TasksHub;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use polyepoxide_core::{Bond, Solvent};
use yup_oauth2::AccessTokenAuthenticator;

use crate::convert::convert_event;
use crate::convert::convert_task;
use crate::error::GoogleError;

type HttpsConnector = hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>;

/// Info about a calendar (for listing)
#[derive(Debug, Clone)]
pub struct CalendarInfo {
    pub id: String,
    pub summary: String,
    pub description: Option<String>,
    pub primary: bool,
}

/// Info about a task list (for listing)
#[derive(Debug, Clone)]
pub struct TaskListInfo {
    pub id: String,
    pub title: String,
}

pub struct GoogleClient {
    calendar_hub: CalendarHub<HttpsConnector>,
    tasks_hub: TasksHub<HttpsConnector>,
}

impl GoogleClient {
    pub async fn new(access_token: &str) -> Result<Self, GoogleError> {
        let auth = AccessTokenAuthenticator::builder(access_token.to_string())
            .build()
            .await
            .map_err(|e| GoogleError::Auth(e.to_string()))?;

        let connector = HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|e| GoogleError::Auth(format!("failed to load native roots: {}", e)))?
            .https_or_http()
            .enable_http2()
            .build();

        let client = Client::builder(TokioExecutor::new()).build(connector);
        let calendar_hub = CalendarHub::new(client.clone(), auth.clone());
        let tasks_hub = TasksHub::new(client, auth);

        Ok(Self {
            calendar_hub,
            tasks_hub,
        })
    }

    /// List all calendars accessible to the user.
    pub async fn list_calendars(&self) -> Result<Vec<CalendarInfo>, GoogleError> {
        let (_, list) = self
            .calendar_hub
            .calendar_list()
            .list()
            .doit()
            .await
            .map_err(|e| GoogleError::Api(e.to_string()))?;

        let calendars = list
            .items
            .unwrap_or_default()
            .into_iter()
            .filter_map(|entry| {
                Some(CalendarInfo {
                    id: entry.id?,
                    summary: entry.summary.unwrap_or_default(),
                    description: entry.description,
                    primary: entry.primary.unwrap_or(false),
                })
            })
            .collect();

        Ok(calendars)
    }

    /// Fetch a calendar and all its events.
    pub async fn fetch_calendar(
        &self,
        calendar_id: &str,
        solvent: &mut Solvent,
    ) -> Result<Bond<Calendar>, GoogleError> {
        // Get calendar metadata
        let (_, cal) = self
            .calendar_hub
            .calendars()
            .get(calendar_id)
            .doit()
            .await
            .map_err(|e| GoogleError::Api(e.to_string()))?;

        // Fetch all events with pagination
        let mut events: Vec<Bond<CalendarEvent>> = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut request = self
                .calendar_hub
                .events()
                .list(calendar_id)
                .single_events(false)
                .max_results(250);

            if let Some(ref token) = page_token {
                request = request.page_token(token);
            }

            let (_, event_list) = request
                .doit()
                .await
                .map_err(|e| GoogleError::Api(e.to_string()))?;

            if let Some(items) = event_list.items {
                for event in items {
                    match convert_event(&event, solvent) {
                        Ok(converted) => events.push(solvent.bond(converted)),
                        Err(e) => {
                            // Log but continue - don't fail entire calendar for one bad event
                            eprintln!("Warning: skipping event: {}", e);
                        }
                    }
                }
            }

            page_token = event_list.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        let calendar = Calendar {
            name: cal.summary.unwrap_or_default(),
            description: cal.description,
            events,
            todos: vec![],
            freebusy: None,
        };

        Ok(solvent.bond(calendar))
    }

    /// List all task lists.
    pub async fn list_task_lists(&self) -> Result<Vec<TaskListInfo>, GoogleError> {
        let (_, list) = self
            .tasks_hub
            .tasklists()
            .list()
            .doit()
            .await
            .map_err(|e| GoogleError::Api(e.to_string()))?;

        let task_lists = list
            .items
            .unwrap_or_default()
            .into_iter()
            .filter_map(|entry| {
                Some(TaskListInfo {
                    id: entry.id?,
                    title: entry.title.unwrap_or_default(),
                })
            })
            .collect();

        Ok(task_lists)
    }

    /// Fetch all tasks from a task list.
    pub async fn fetch_tasks(
        &self,
        task_list_id: &str,
        solvent: &mut Solvent,
    ) -> Result<Vec<Bond<CalendarTodo>>, GoogleError> {
        let mut todos: Vec<Bond<CalendarTodo>> = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut request = self
                .tasks_hub
                .tasks()
                .list(task_list_id)
                .max_results(100)
                .show_completed(true)
                .show_hidden(true);

            if let Some(ref token) = page_token {
                request = request.page_token(token);
            }

            let (_, task_list) = request
                .doit()
                .await
                .map_err(|e| GoogleError::Api(e.to_string()))?;

            if let Some(items) = task_list.items {
                for task in items {
                    match convert_task(&task) {
                        Ok(converted) => todos.push(solvent.bond(converted)),
                        Err(e) => {
                            eprintln!("Warning: skipping task: {}", e);
                        }
                    }
                }
            }

            page_token = task_list.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        Ok(todos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires GOOGLE_ACCESS_TOKEN env var
    async fn test_list_calendars() {
        let token = std::env::var("GOOGLE_ACCESS_TOKEN").expect("GOOGLE_ACCESS_TOKEN not set");
        let client = GoogleClient::new(&token).await.unwrap();
        let calendars = client.list_calendars().await.unwrap();
        println!("Found {} calendars", calendars.len());
        for cal in &calendars {
            println!("  {} ({})", cal.summary, cal.id);
        }
        assert!(!calendars.is_empty());
    }

    #[tokio::test]
    #[ignore] // Requires GOOGLE_ACCESS_TOKEN env var
    async fn test_fetch_primary_calendar() {
        let token = std::env::var("GOOGLE_ACCESS_TOKEN").expect("GOOGLE_ACCESS_TOKEN not set");
        let client = GoogleClient::new(&token).await.unwrap();
        let mut solvent = Solvent::new();
        let calendar = client.fetch_calendar("primary", &mut solvent).await.unwrap();
        let cal = calendar.value().unwrap();
        println!("Calendar: {}", cal.name);
        println!("Events: {}", cal.events.len());
    }

    #[tokio::test]
    #[ignore] // Requires GOOGLE_ACCESS_TOKEN env var
    async fn test_list_task_lists() {
        let token = std::env::var("GOOGLE_ACCESS_TOKEN").expect("GOOGLE_ACCESS_TOKEN not set");
        let client = GoogleClient::new(&token).await.unwrap();
        let task_lists = client.list_task_lists().await.unwrap();
        println!("Found {} task lists", task_lists.len());
        for tl in &task_lists {
            println!("  {} ({})", tl.title, tl.id);
        }
    }
}
