use std::path::PathBuf;

use clap::{Args, Parser};
use crate::cli::Config;

mod forward;
mod common;
pub(crate) mod macros;
mod utils;
mod serve;
pub(crate) mod cli;
mod large_state;
mod maybe_async;

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt().init();
	let cfg = Config::parse();
	match cfg {
		c @ Config::Forward { .. } => forward::handle(c).await,
		c @ Config::Serve { .. } => serve::serve_dir(c).await,
		_ => {}
	};
}
