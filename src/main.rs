//! Self-Evolved Agent (SEA) - A CLI tool for interacting with LLMs
//!
//! This is the main entry point for the SEA application.

mod api;
mod cli;
mod config;
mod persona;
mod session;
mod tools;
mod utils;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    // Set up logging
    utils::setup_logging();

    let cli = cli::Cli::parse();

    match &cli.command {
        Some(cli::Commands::Query { message, model, temperature }) => {
            cli::run_query(message.clone(), model.clone(), *temperature).await?;
        }
        Some(cli::Commands::Chat) => {
            cli::run_chat().await?;
        }
        Some(cli::Commands::Config { command }) => {
            match command {
                cli::ConfigCommands::Set { key, value } => {
                    cli::config_set(key.clone(), value.clone())?;
                }
                cli::ConfigCommands::Get { key } => {
                    cli::config_get(key.clone())?;
                }
                cli::ConfigCommands::List => {
                    cli::config_list()?;
                }
                cli::ConfigCommands::Reset => {
                    cli::config_reset()?;
                }
            }
        }
        Some(cli::Commands::Models) => {
            cli::run_list_models().await?;
        }
        Some(cli::Commands::Tools) => {
            cli::run_list_tools()?;
        }
        Some(cli::Commands::Session { command }) => {
            match command {
                cli::SessionCommands::List => {
                    cli::run_list_sessions()?;
                }
                cli::SessionCommands::Delete { session_id } => {
                    cli::run_delete_session(session_id.clone())?;
                }
            }
        }
        Some(cli::Commands::Persona { command }) => {
            match command {
                cli::PersonaCommands::List => {
                    cli::run_list_personas()?;
                }
                cli::PersonaCommands::Show { name } => {
                    cli::run_show_persona(&name)?;
                }
                cli::PersonaCommands::Create { name, template } => {
                    cli::run_create_persona(&name, &template)?;
                }
                cli::PersonaCommands::Reload => {
                    cli::run_reload_personas()?;
                }
                cli::PersonaCommands::Stats => {
                    cli::run_persona_stats()?;
                }
            }
        }
        None => {
            // No command specified, print help
            let _ = cli::Cli::parse_from(["sea", "--help"]);
        }
    }

    Ok(())
}
