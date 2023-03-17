use clap::Parser;

use crate::cli::Config;

mod common;
pub(crate) mod macros;
mod utils;
pub(crate) mod cli;
mod large_state;
mod maybe_async;
mod command;

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt().init();
	let cfg = Config::parse();
	command::run(cfg).await
}
