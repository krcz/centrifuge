use std::sync::Arc;

use aldehyde_cal::Calendar;
use clap::Subcommand;
use polyepoxide_core::{Bond, Catalogue, Cell, DynBond, Oxide, Store, key_from_cid};
use silane_goog::GoogleClient;

use crate::config::resolve_gcal_import_bookmark;
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

pub async fn run(
    mut ctx: AppContext,
    access_token: &str,
    action: GcalAction,
) -> anyhow::Result<()> {
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
            let bookmark = resolve_gcal_import_bookmark();
            println!("Fetching calendar '{}'...", calendar_id);
            let calendar_bond = client
                .fetch_calendar(&calendar_id, &mut ctx.solvent)
                .await?;

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
                        let (cid, catalogue_cid) =
                            store_calendar(&ctx, &calendar_cell, &bookmark, &calendar_id)?;
                        let cal = calendar_cell.value();
                        println!(
                            "Imported '{}': {} events, {} tasks",
                            cal.name,
                            cal.events.len(),
                            cal.todos.len()
                        );
                        println!("CID: {}", cid);
                        println!("Catalogue: {} ({})", bookmark, catalogue_cid);
                        println!("Entry: {}", calendar_id);
                        return Ok(());
                    }
                }
            }

            let calendar_cell = calendar_bond.cell().unwrap().clone();
            let (cid, catalogue_cid) =
                store_calendar(&ctx, &calendar_cell, &bookmark, &calendar_id)?;
            let cal = calendar_cell.value();
            println!(
                "Imported '{}': {} events, {} tasks",
                cal.name,
                cal.events.len(),
                cal.todos.len()
            );
            println!("CID: {}", cid);
            println!("Catalogue: {} ({})", bookmark, catalogue_cid);
            println!("Entry: {}", calendar_id);
        }
    }

    Ok(())
}

fn store_calendar(
    ctx: &AppContext,
    calendar_cell: &Arc<Cell<Calendar>>,
    bookmark: &str,
    calendar_id: &str,
) -> anyhow::Result<(cid::Cid, cid::Cid)> {
    let (cid, _) = ctx.solvent.persist_cell(calendar_cell, &ctx.store)?;
    let dyn_bond = DynBond::from_typed(Bond::from_cell(calendar_cell.clone()));
    let mut catalogue = load_catalogue(&ctx.store, bookmark)?;
    catalogue.insert(calendar_id.to_string(), dyn_bond);

    let catalogue_cell = ctx.solvent.add(catalogue);
    let (catalogue_cid, _) = ctx.solvent.persist_cell(&catalogue_cell, &ctx.store)?;
    let catalogue_bond = DynBond::from_typed(Bond::from_cell(catalogue_cell));
    ctx.store.put_bookmark(bookmark, &catalogue_bond)?;
    Ok((cid, catalogue_cid))
}

fn load_catalogue<S: Store>(store: &S, bookmark_name: &str) -> anyhow::Result<Catalogue> {
    let Some(bookmark) = store.get_bookmark(bookmark_name)? else {
        return Ok(Catalogue::new());
    };

    if !bookmark.matches_schema::<Catalogue>() {
        anyhow::bail!("bookmark `{}` does not point to a Catalogue", bookmark_name);
    }

    let key = key_from_cid(&bookmark.cid());
    let bytes = store.get(&key)?.ok_or_else(|| {
        anyhow::anyhow!("missing Catalogue bytes for bookmark `{}`", bookmark_name)
    })?;
    Ok(Catalogue::from_bytes(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyepoxide_core::{MemoryStore, Solvent};

    #[test]
    fn load_catalogue_returns_empty_when_bookmark_missing() {
        let store = MemoryStore::new();
        let catalogue = load_catalogue(&store, "calendars").unwrap();
        assert!(catalogue.is_empty());
    }

    #[test]
    fn load_catalogue_decodes_existing_bookmark() {
        let solvent = Solvent::new();
        let store = MemoryStore::new();

        let mut catalogue = Catalogue::new();
        catalogue.insert(
            "primary".to_string(),
            DynBond::from_typed(Bond::new("calendar".to_string())),
        );

        let catalogue_cell = solvent.add(catalogue);
        solvent.persist_cell(&catalogue_cell, &store).unwrap();
        let bookmark = DynBond::from_typed(Bond::from_cell(catalogue_cell));
        store.put_bookmark("calendars", &bookmark).unwrap();

        let loaded = load_catalogue(&store, "calendars").unwrap();
        assert!(loaded.contains_key("primary"));
    }
}
