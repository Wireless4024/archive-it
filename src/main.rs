use std::path::PathBuf;

use clap::{Args, Parser};
use crate::cli::Config;

mod forward;
mod common;
pub(crate) mod macros;
mod utils;
mod serve;
pub(crate) mod cli;

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt().init();
	//extend_absolute(br#"<a href="/abcd/dsads/dsadsa/abc.js"></a>"#, "".as_ref(), &mut Vec::new());
	let cfg = Config::parse();
	match cfg {
		c @ Config::Forward { .. } => forward::handle(c).await,
		c @ Config::Serve { .. } => serve::serve_dir(c).await,
		_ => {}
	};
}
