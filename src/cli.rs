#[cfg(feature = "zip")]
use std::fmt::{Debug, Display, Formatter};
use std::path::PathBuf;

use clap::{Args, Parser};
#[cfg(feature = "zip")]
use clap::ValueEnum;

#[derive(Parser)]
#[command(version, long_about = None)]
pub(crate) enum Config {
	/// Forward traffic from local interface to upstream
	Forward {
		/// secure upstream
		#[arg(short, long, default_value_t = true)]
		secure: bool,
		/// upstream host
		host: String,
		/// output dir
		output: PathBuf,
		/// listen port
		#[clap(flatten)]
		http: HttpConfig,
		/// provide value to replace upstream host with
		#[arg(short, long)]
		prefix_local: Option<String>,
		// replace url that start with / to relative path
		//#[arg(short, long)]
		//rewrite_prefix: bool,
	},
	/// Serve local content without forwarding to upstream
	Serve {
		/// Path to archive folder
		path: String,
		#[clap(flatten)]
		http: HttpConfig,
	},
	#[cfg(feature = "zip")]
	/// Compress content into single file
	Compress {
		/// Path to archive folder
		path: String,
		/// Compress format
		#[arg(short, long, default_value_t = CompressFormat::zip)]
		format: CompressFormat,
		/// Output file
		output: Option<String>,
	},
}

#[cfg(feature = "zip")]
#[derive(ValueEnum, Clone, Debug)]
pub(crate) enum CompressFormat {
	#[cfg(feature = "zip")]
	zip
}
#[cfg(feature = "zip")]
impl Display for CompressFormat {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		Debug::fmt(self, f)
	}
}
#[cfg(feature = "zip")]
impl CompressFormat {
	pub const fn ext(&self) -> &'static str {
		match self { CompressFormat::zip => { ".zip" } }
	}
}

#[derive(Args)]
pub(crate) struct HttpConfig {
	#[arg(short, long, default_value_t = 3000)]
	/// Port to listen
	pub listen: u16,

	/// Url to rewrite into (default is localhost:port, currently unsupported)
	#[arg(short, long)]
	pub rewrite: Option<String>,
}