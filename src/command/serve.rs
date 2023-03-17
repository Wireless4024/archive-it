use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Extension;
use axum::extract::{Path, Query, RawBody};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::http::header::CONTENT_TYPE;
use axum::response::Response;
#[cfg(all(feature = "serve-archive", feature = "piz"))]
use futures_util::Stream;
#[cfg(all(feature = "serve-archive", feature = "piz"))]
use piz::read::FileTree;
use tracing::info;

use crate::{Config, http_all};
use crate::common::{normalize_url_path, serve_file, StreamBodyExt, StreamResponse};
use crate::state::HttpState;

pub(crate) async fn serve_dir(config: Config) {
	let Config::Serve { path, mut http } = config else { unreachable!() };
	let listen = http.listen;
	http.rewrite = Some(http.rewrite.unwrap_or_else(|| format!("localhost:{listen}")));

	#[cfg(all(feature = "serve-archive", feature = "piz"))]
		let (typ, source) = if path.ends_with(".zip") {
		(ServeType::Zip, Some(Arc::new(ZipSource::new(path.as_ref()).unwrap())))
	} else {
		(ServeType::Direct, None)
	};

	#[cfg(not(all(feature = "serve-archive", feature = "piz")))]
		let (typ, source) = (ServeType::Direct, None);
	let source: Option<Arc<ZipSource>> = source;
	http_all!(http.listen, serve_proxy, serve_root, Arc::new(ServeConfig{path,typ,rewrite:http.rewrite.unwrap()}),source);
}

struct ServeConfig {
	path: String,
	rewrite: String,
	typ: ServeType,
}

enum ServeType {
	Direct,
	#[cfg(all(feature = "serve-archive", feature = "piz"))]
	Zip,
}

async fn serve_root(header: HeaderMap,
                    extension: Extension<Arc<ServeConfig>>,
                    source: Extension<Option<Arc<ZipSource>>>,
                    q: Query<HashMap<String, String>>,
                    payload: RawBody) -> StreamResponse {
	serve_proxy(header, Path(String::new()), q, extension, source, payload).await
}

async fn serve_proxy(header: HeaderMap,
                     Path(path): Path<String>,
                     Query(query): Query<HashMap<String, String>>,
                     Extension(cfg): Extension<Arc<ServeConfig>>,
                     Extension(source): Extension<Option<Arc<ZipSource>>>,
                     RawBody(_): RawBody) -> StreamResponse {
	let method = Method::from_bytes(header.get("method").unwrap_or(&HeaderValue::from_static("GET")).as_bytes()).unwrap_or_default();
	let state = HttpState {
		method,
		query,
	};
	let npath = normalize_url_path(cfg.path.as_ref(), &state, &path, true);
	match cfg.typ {
		ServeType::Direct => {
			serve_file(npath, Response::builder()).await
		}
		#[cfg(all(feature = "serve-archive", feature = "piz"))]
		ServeType::Zip => {
			info!("serving from zip file \"{}!/{path}\"", cfg.path);
			serve_zip(cfg.path.as_ref(), npath, Arc::clone(source.as_ref().unwrap())).await
		}
	}
}

#[cfg(all(feature = "serve-archive", feature = "piz"))]
#[repr(C)] // prevent field re-order
struct ZipSource {
	content: piz::read::DirectoryContents<'static>,
	zip: piz::ZipArchive<'static>,
	mmap: memmap::Mmap,
	file: std::fs::File,
}

#[cfg(all(feature = "serve-archive", feature = "piz"))]
impl ZipSource {
	fn new(path: &std::path::Path) -> std::io::Result<Self> {
		let file = std::fs::File::open(path)?;
		use fs4::FileExt;
		file.lock_shared()?;
		let mmap = unsafe { memmap::Mmap::map(&file)? };
		let zip = piz::ZipArchive::new(&mmap).unwrap();
		let zip: piz::ZipArchive<'static> = unsafe { std::mem::transmute(zip) };
		let content = piz::read::as_tree(zip.entries()).unwrap();
		let content = unsafe { std::mem::transmute(content) };
		Ok(Self {
			content,
			file,
			mmap,
			zip,
		})
	}

	fn get(&self, path: &str) -> Option<impl Stream<Item=crate::common::StreamResponseItem> + Send + Sync + 'static> {
		let entry = self.content.lookup(path).ok()?;
		let content = std::sync::Mutex::new(self.zip.read(entry).ok()?);
		let buf = [0; 4096];

		Some(futures_util::stream::unfold((content, buf), |it| async {
			tokio::task::spawn_blocking(|| {
				Self::next_entry(it)
			}).await.unwrap()
		}))
	}

	fn next_entry((content, mut buf): (std::sync::Mutex<Box<dyn std::io::Read + Send>>, [u8; 4096]))
	              -> Option<(crate::common::StreamResponseItem, (std::sync::Mutex<Box<dyn std::io::Read + Send>>, [u8; 4096]))> {
		let mut reader = content.lock().ok()?;

		let len = match reader.read(&mut buf) {
			Ok(n) => {
				n
			}
			Err(err) => {
				drop(reader);
				return Some((Err(axum::Error::new(err)), (content, buf)));
			}
		};
		let out = bytes::Bytes::copy_from_slice(&buf[..len]);
		drop(reader);
		Some((Ok(out), (content, buf)))
	}
}

#[cfg(all(feature = "serve-archive", feature = "piz"))]
async fn serve_zip(root: &std::path::Path, npath: PathBuf, zip: Arc<ZipSource>) -> StreamResponse {
	let path = npath.strip_prefix(root).unwrap_or(&npath).to_string_lossy();
	let typ = mime_guess::from_path(&npath);
	let builder = Response::builder().header(CONTENT_TYPE, typ.first().unwrap_or(mime_guess::mime::TEXT_HTML).to_string());
	
	if let Some(res) = zip.get(&path) {
		builder.stream(res)
	} else {
		builder
			.status(StatusCode::NOT_FOUND)
			.stream(futures_util::stream::empty())
	}
}