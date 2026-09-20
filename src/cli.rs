// use crate::config::Config;
// use clap::{Parser, Subcommand};
// use std::path::PathBuf;
//
// #[derive(Debug, Parser)]
// #[command(name = "blue")]
// #[command(about = "Blue project tool")]
// struct Cli {
//     #[command(subcommand)]
//     command: Command,
// }
//
// #[derive(Debug, Subcommand)]
// enum Command {
//     /// Validate a project TOML config file
//     Validate {
//         /// Path to the project TOML file
//         config: PathBuf,
//     },
// }
//
// pub fn run<I, T>(args: I) -> Result<(), Box<dyn std::error::Error>>
// where
//     I: IntoIterator<Item = T>,
//     T: Into<std::ffi::OsString> + Clone,
// {
//     let cli = Cli::parse_from(args);
//
//     match cli.command {
//         Command::Validate { config } => {
//             let config = Config::load(config)?;
//             println!("Config: {:#?}", config.graph);
//
//             println!("Provider Registry: {:#?}", config.provider_registry);
//
//             println!("validated");
//         }
//     }
//
//     Ok(())
// }
