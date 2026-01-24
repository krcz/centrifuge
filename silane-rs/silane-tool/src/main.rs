mod config;
mod error;
mod gcal;
mod store;

#[cfg(feature = "chat")]
mod chat;

use std::path::PathBuf;
use std::str::FromStr;

use cid::Cid;
use clap::{Parser, Subcommand};
use silane_openrouter::OpenRouterClient;

use crate::config::{load_api_key, load_google_token, resolve_store_config};
use crate::gcal::GcalAction;
use crate::store::{AppContext, StoreType};

#[derive(Parser)]
#[command(name = "sih")]
#[command(about = "Silane tools for LLM interaction", long_about = None)]
struct Cli {
    /// Store type: fjall or rocks
    #[arg(long, global = true)]
    store_type: Option<StoreType>,

    /// Path to the polyepoxide store
    #[arg(long, global = true)]
    store: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[cfg(feature = "chat")]
    /// Start an interactive chat session
    Chat {
        /// Continue from existing conversation (Message CID)
        #[arg(long)]
        continue_from: Option<String>,

        /// Model to use
        #[arg(short, long, default_value = "openai/gpt-4o")]
        model: String,

        /// Reasoning effort: low, medium, high
        #[arg(long)]
        reasoning: Option<String>,
    },

    /// Google Calendar operations
    Gcal {
        #[command(subcommand)]
        action: GcalAction,

        /// Override access token
        #[arg(long, global = true)]
        access_token: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let (store_type, store_path) = resolve_store_config(cli.store_type, cli.store);
    let ctx = AppContext::open(store_type, store_path)?;

    match cli.command {
        #[cfg(feature = "chat")]
        Command::Chat {
            continue_from,
            model,
            reasoning,
        } => {
            let api_key = load_api_key()?;
            let client = OpenRouterClient::new(api_key);

            let continue_cid = continue_from
                .map(|s| Cid::from_str(&s))
                .transpose()?;

            chat::run(ctx, client, model, reasoning, continue_cid).await?;
        }
        Command::Gcal {
            action,
            access_token,
        } => {
            let token = access_token
                .map(Ok)
                .unwrap_or_else(load_google_token)?;
            gcal::run(ctx, &token, action).await?;
        }
    }

    Ok(())
}
