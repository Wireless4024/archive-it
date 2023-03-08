use std::future::Future;
use std::hash::Hasher;
use std::io;
use std::path::PathBuf;

use rand::distributions::Alphanumeric;
use rand::Rng;
use tokio::fs::{create_dir_all, File, rename};
use tokio::io::AsyncWriteExt;
use twox_hash::xxh3::Hash64;

use crate::maybe_async::MaybeAsync;
use crate::unwrap_void;

pub struct LargeState {
	hasher: Hash64,
	file: Option<(PathBuf, File)>,
	corrupted: bool,
	out_dir: PathBuf,
}

impl LargeState {
	pub async fn new(out_dir: PathBuf, save_state: bool) -> io::Result<Self> {
		let file = if save_state {
			let state_path: String = rand::thread_rng()
				.sample_iter(Alphanumeric)
				.take(8)
				.map(char::from)
				.collect();
			let path = out_dir.join(state_path);
			let file = File::create(&path).await?;
			Some((path, file))
		} else {
			None
		};
		Ok(Self {
			hasher: Hash64::default(),
			out_dir,
			file,
			corrupted: false,
		})
	}

	pub fn push_bytes<'a, 'b: 'a>(&'a mut self, bytes: &'b [u8]) -> MaybeAsync<io::Result<()>, impl Future<Output=io::Result<()>> + 'a> {
		if self.corrupted { return MaybeAsync::Sync(Ok(())); }
		self.hasher.write(bytes);
		if let Some((_, file)) = &mut self.file {
			MaybeAsync::Async(file.write_all(bytes))
		} else {
			MaybeAsync::Sync(Ok(()))
		}
	}

	pub async fn finish(self) -> Option<String> {
		if self.corrupted { return None; }
		let hash = format!("{:x}", self.hasher.finish());
		unwrap_void!(create_dir_all(&self.out_dir).await);
		if let Some((path, mut file)) = self.file {
			unwrap_void!(file.shutdown().await);
			let state_path = self.out_dir.join(format!("{hash}.state"));
			unwrap_void!(rename(path,state_path).await);
		}
		Some(hash)
	}
}