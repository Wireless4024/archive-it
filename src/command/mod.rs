use crate::cli::Config;

pub(crate) mod forward;
pub(crate) mod serve;
#[cfg(feature = "zip")]
pub(crate) mod compress;

pub(crate) async fn run(cfg: Config) {
	match cfg {
		c @ Config::Forward { .. } => forward::handle(c).await,
		c @ Config::Serve { .. } => serve::serve_dir(c).await,
		#[cfg(feature = "zip")]
		c @ Config::Compress { .. } => compress::dir(c).await,
		_ => {}
	}
}