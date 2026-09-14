mod app;
mod cli;
mod podcast;
mod tui;
mod ui;

use anyhow::Result;
use clap::{CommandFactory, Parser};

use cli::Cli;
use podcast::{DefaultMode, load_config};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(cmd) => cli::run(cmd).await,
        None => {
            let (config, freshly_created) = load_config()?;
            let mode = if cli.tui {
                DefaultMode::Tui
            } else if cli.cli {
                DefaultMode::Cli
            } else {
                config.default_mode
            };
            match mode {
                DefaultMode::Tui => launch_tui(config, freshly_created).await,
                DefaultMode::Cli => {
                    Cli::command().print_help()?;
                    println!();
                    Ok(())
                }
            }
        }
    }
}

async fn launch_tui(config: podcast::Config, freshly_created: bool) -> Result<()> {
    let config_notice = if freshly_created {
        Some(format!(
            "Config initialized with demo podcasts — downloads go to {} — press 'c' to edit config",
            config.download_dir_location
        ))
    } else {
        None
    };
    tui::run(config, config_notice).await
}
