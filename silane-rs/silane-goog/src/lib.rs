//! Google Calendar and Tasks API client for the polyepoxide ecosystem.
//!
//! This crate provides integration between Google Calendar/Tasks APIs and
//! the aldehyde-cal data model.
//!
//! # Example
//!
//! ```ignore
//! use polyepoxide_core::Solvent;
//! use silane_goog::GoogleClient;
//!
//! #[tokio::main]
//! async fn main() {
//!     let client = GoogleClient::new("your-access-token").await.unwrap();
//!     let mut solvent = Solvent::new();
//!
//!     // List available calendars
//!     let calendars = client.list_calendars().await.unwrap();
//!     for cal in &calendars {
//!         println!("{}: {}", cal.id, cal.summary);
//!     }
//!
//!     // Fetch the primary calendar
//!     let calendar = client.fetch_calendar("primary", &mut solvent).await.unwrap();
//!     let cal = calendar.value().unwrap();
//!     println!("Found {} events", cal.events.len());
//! }
//! ```

mod client;
mod convert;
mod error;
mod rrule;

pub use client::{CalendarInfo, GoogleClient, TaskListInfo};
pub use error::GoogleError;
