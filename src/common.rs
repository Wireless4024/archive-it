use std::path::{Path, PathBuf};
use std::pin::Pin;

use axum::body::{Bytes, StreamBody};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::http::response::Builder;
use axum::response::Response;
use bytes::BytesMut;
use futures_util::Stream;
use futures_util::stream::unfold;
use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use tokio::fs::{File, read_link};
use tokio::io::AsyncReadExt;

use crate::state::HttpState;
use crate::stream_single;

pub type StreamResponseItem = Result<Bytes, axum::Error>;
pub type StreamResponseType = Pin<Box<dyn Stream<Item=StreamResponseItem> + Send + Sync>>;
pub type StreamResponse = Response<StreamBody<StreamResponseType>>;

pub trait StreamBodyExt {
	fn stream_single<B: Into<Bytes>>(self, buf: B) -> StreamResponse;
	fn stream<S: Stream<Item=StreamResponseItem> + Send + Sync + 'static>(self, stream: S) -> StreamResponse;
}

impl StreamBodyExt for Builder {
	fn stream_single<B: Into<Bytes>>(self, buf: B) -> StreamResponse {
		self.body(StreamBody::new(stream_single!(buf.into()))).unwrap()
	}

	fn stream<S: Stream<Item=StreamResponseItem> + Send + Sync + 'static>(self, stream: S) -> StreamResponse {
		let stream: StreamResponseType = Box::pin(stream);
		self.body(StreamBody::new(stream)).unwrap()
	}
}

static UNKNOWN_EXT: &str = "unknown_ext";

pub(crate) fn normalize_url_path(output: &Path, state: &HttpState, path: &str, with_state: bool) -> PathBuf {
	if path.is_empty() {
		output.join(format!("index.{}.{UNKNOWN_EXT}", state.method))
	} else {
		let mut path = output.join(path);

		if path.as_os_str().to_string_lossy().ends_with('/') || path.extension().is_none() {
			if let Some(name) = path.file_name() {
				let mut last = name.to_os_string();
				last.push(UNKNOWN_EXT);
				path.pop();
				path.push(last);
			}
		}
		if !state.query.is_empty() && with_state {
			let name = if let Some(name) = path.file_stem() {
				name
			} else {
				path.file_name().unwrap()
			};
			let mut last = name.to_os_string();
			let ext = path.extension();
			for (k, v) in state.query.iter() {
				last.push("-");
				last.push(percent_encode(k.as_bytes(), NON_ALPHANUMERIC).to_string());
				if !v.is_empty() {
					last.push("=");
					last.push(percent_encode(v.as_bytes(), NON_ALPHANUMERIC).to_string());
				}
			}
			if let Some(ext) = ext {
				last.push(".");
				last.push(ext);
			}
			path.pop();
			path.push(last);
		}
		path
	}
}

pub(crate) async fn serve_file(npath: PathBuf, mut builder: Builder) -> StreamResponse {
	let actual = read_link(&npath).await.unwrap_or(npath);
	let typ = mime_guess::from_path(&actual);
	let Ok(mut file) = File::open(&actual).await else {
		return builder.status(StatusCode::NOT_FOUND).stream_single(vec![]);
	};
	let meta = file.metadata().await.unwrap();
	builder = builder.header(CONTENT_TYPE, typ.first().unwrap_or(mime_guess::mime::TEXT_HTML).to_string());

	if meta.len() > (1 << 18) {
		builder.stream(unfold((file, BytesMut::with_capacity(4096)), |(mut file, mut buf)| async move {
			buf.reserve(4096);
			match file.read_buf(&mut buf).await {
				Ok(0) => {
					None
				}
				Ok(n) => {
					Some((Ok(buf.split_to(n).freeze()), (file, buf)))
				}
				Err(err) => {
					Some((Err(axum::Error::new(err)), (file, buf)))
				}
			}
		}))
	} else {
		let mut buf = Vec::with_capacity(meta.len() as usize);
		file.read_to_end(&mut buf).await.unwrap();
		builder.stream_single(buf)
	}
}