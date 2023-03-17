use std::fs::File;
use std::path::{Path, PathBuf};

use tracing::info;
use zip::write::FileOptions;
use zip::ZipWriter;

use crate::cli::{CompressFormat, Config};
use crate::utils::read_dir_recursive;

pub(crate) async fn dir(cfg: Config) {
	let Config::Compress { path, format, output } = cfg else { unreachable!() };
	let path: &Path = path.as_ref();
	let output: PathBuf = if let Some(mut output) = output {
		if !output.ends_with(format.ext()) {
			output.push_str(format.ext());
		}
		PathBuf::from(output)
	} else {
		let mut name = path.canonicalize().expect("Output dir").file_name().unwrap().to_os_string();
		name.push(format.ext());
		PathBuf::from(name)
	};
	match format {
		CompressFormat::zip => {
			compress_zip(path, &output)
		}
	}
	info!("finished compress folder {output:?}");
}

pub fn compress_zip(dir: &Path, output: &Path) {
	let out_file = File::create(output).unwrap();
	let mut writer = ZipWriter::new(out_file);
	let root = dir;
	let option = FileOptions::default()
		.compression_level(Some(9));
	for x in read_dir_recursive(dir) {
		let path = x.strip_prefix(root).unwrap();
		info!("compressing {path:?}");
		writer.start_file(path.to_string_lossy().into_owned(), option).unwrap();
		let mut f = File::open(x).unwrap();
		std::io::copy(&mut f, &mut writer).unwrap();
	}
	writer.finish().expect("Finish writing zip").sync_all().unwrap();
}