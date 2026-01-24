use clap::Subcommand;
use silane_goog::GoogleClient;

use crate::store::AppContext;

#[derive(Subcommand)]
pub enum GcalAction {
    /// List available calendars
    List,
    /// Import a calendar into the store
    Import {
        /// Calendar ID to import (default: "primary")
        #[arg(long, default_value = "primary")]
        calendar_id: String,

        /// Also import tasks from Google Tasks
        #[arg(long)]
        with_tasks: bool,
    },
}

pub async fn run(mut ctx: AppContext, access_token: &str, action: GcalAction) -> anyhow::Result<()> {
    let client = GoogleClient::new(access_token).await?;

    match action {
        GcalAction::List => {
            let calendars = client.list_calendars().await?;
            if calendars.is_empty() {
                println!("No calendars found.");
                return Ok(());
            }
            println!("Available calendars:");
            for cal in calendars {
                let primary = if cal.primary { " (primary)" } else { "" };
                println!("  {} - {}{}", cal.id, cal.summary, primary);
            }
        }
        GcalAction::Import {
            calendar_id,
            with_tasks,
        } => {
            println!("Fetching calendar '{}'...", calendar_id);
            let calendar_bond = client.fetch_calendar(&calendar_id, &mut ctx.solvent).await?;

            if with_tasks {
                let task_lists = client.list_task_lists().await?;
                if !task_lists.is_empty() {
                    println!("Fetching tasks...");
                    let mut all_todos = Vec::new();
                    for tl in &task_lists {
                        let todos = client.fetch_tasks(&tl.id, &mut ctx.solvent).await?;
                        all_todos.extend(todos);
                    }
                    if !all_todos.is_empty() {
                        let mut calendar = calendar_bond.value().unwrap().clone();
                        calendar.todos = all_todos;
                        let calendar_cell = ctx.solvent.add(calendar);
                        let (cid, _) = ctx.solvent.persist_cell(&calendar_cell, &ctx.store)?;
                        let cal = calendar_cell.value();
                        println!(
                            "Imported '{}': {} events, {} tasks",
                            cal.name,
                            cal.events.len(),
                            cal.todos.len()
                        );
                        println!("CID: {}", cid);
                        return Ok(());
                    }
                }
            }

            let calendar_cell = calendar_bond.cell().unwrap();
            let (cid, _) = ctx.solvent.persist_cell(calendar_cell, &ctx.store)?;
            let cal = calendar_cell.value();
            println!(
                "Imported '{}': {} events, {} tasks",
                cal.name,
                cal.events.len(),
                cal.todos.len()
            );
            println!("CID: {}", cid);
        }
    }

    Ok(())
}
