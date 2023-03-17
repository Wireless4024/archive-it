use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::hard_link;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use axum::Extension;
use axum::extract::{Path, Query, RawBody};
use axum::http::{HeaderMap, HeaderValue, Method};
use axum::http::header::{ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_LENGTH, CONTENT_TYPE, STRICT_TRANSPORT_SECURITY};
use axum::response::Response;
use bstr::ByteSlice;
use futures_util::stream::unfold;
use futures_util::StreamExt;
use reqwest::Client;
use tokio::fs::{create_dir_all, File, write};
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use crate::{Config, http_all, unwrap_void};
use crate::cli::HttpConfig;
use crate::common::{normalize_url_path, serve_file, StreamBodyExt, StreamResponse};

struct ForwardConfig {
	secure: bool,
	host: String,
	output: PathBuf,
	http: HttpConfig,
	prefix_local: Option<String>,
}

pub(super) async fn handle(cfg: Config) {
	if let Config::Forward { secure, host, output, mut http, prefix_local } = cfg {
		let listen = http.listen;
		http.rewrite = Some(http.rewrite.unwrap_or_else(|| format!("localhost:{listen}")));

		http_all!(listen, get_proxy, get_root, ForwardConfig { secure, host, output, http, prefix_local });
	};
}

async fn get_root(header: HeaderMap,
                  extension: Extension<Arc<ForwardConfig>>,
                  q: Query<HashMap<String, String>>,
                  payload: RawBody) -> StreamResponse {
	get_proxy(header, Path(String::new()), q, extension, payload).await
}

async fn get_proxy(header: HeaderMap,
                   Path(path): Path<String>,
                   Query(q): Query<HashMap<String, String>>,
                   Extension(cfg): Extension<Arc<ForwardConfig>>,
                   RawBody(payload): RawBody) -> StreamResponse {
	let method = Method::from_bytes(header.get("method").unwrap_or(&HeaderValue::from_static("GET")).as_bytes()).unwrap_or_default();
	let npath = normalize_url_path(&cfg.output, &method, &q, &path, cfg.prefix_local.is_none());
	if let Some(parent) = npath.parent() {
		if let Err(e) = create_dir_all(parent).await {
			warn!("{e} at {parent:?}");
		}
	}
	let builder = Response::builder();
	if method == Method::GET && npath.exists() {
		info!("Serving:    {path:?}");
		serve_file(npath, builder).await
	} else {
		info!("Forwarding: {path:?}");
		let resp = Client::new()
			.request(method.clone(), format!("{}://{}/{path}", if cfg.secure { "https" } else { "http" }, cfg.host))
			.body(payload)
			.query(&q)
			.send()
			.await
			.unwrap();
		#[allow(clippy::never_loop)]
			let (out, npath, link) = loop {
			if npath.extension().and_then(|it| if it == "unknown_ext" { Some(()) } else { None }).is_some() {
				if let Some(ct) = resp.headers().get(CONTENT_TYPE) {
					if let Some(ext) = mime_guess::get_mime_extensions_str(&String::from_utf8_lossy(ct.as_bytes())) {
						let mut f = npath.file_stem().unwrap().to_os_string();
						f.push(".");
						f.push(ext[0]);
						break (Cow::<std::path::Path>::Owned(npath.parent().unwrap().join(f)), Cow::<std::path::Path>::Borrowed(&npath), true);
					}
				}
			}
			break (Cow::<std::path::Path>::Borrowed(&npath), Cow::<std::path::Path>::Borrowed(&npath), false);
		};
		#[allow(clippy::never_loop)]
		loop {
			let mut builder = Response::builder()
				.header(ACCESS_CONTROL_ALLOW_ORIGIN, format!("localhost:{}", cfg.http.listen))
				.header(ACCESS_CONTROL_ALLOW_CREDENTIALS, "true")
				.header(ACCESS_CONTROL_ALLOW_METHODS, "*");
			for (k, v) in resp.headers().iter() {
				if matches!(k,&CONTENT_LENGTH|&STRICT_TRANSPORT_SECURITY) { continue; }
				if k.as_str() == "expect-ct" { continue; }
				builder = builder.header(k, v);
			}


			if let Some(ct) = resp.headers().get(CONTENT_TYPE) {
				let ct = ct.as_bytes();
				match ct {
					| b"text/css"
					| b"text/javascript"
					| b"application/json"
					| b"text/html"
					| b"application/xhtml+xml"
					=> {
						let resp_len = resp.content_length().unwrap_or(512);
						let body: Bytes = resp.bytes().await.unwrap_or_default();
						let mut target = Vec::with_capacity(resp_len as usize);
						let mut parts = body.as_bstr().split_str(&cfg.host);
						if let Some(x) = parts.next() {
							target.extend(x);
						}
						for mut x in parts {
							if cfg.prefix_local.is_some() && x.ends_with(b"//") {
								if x.ends_with(b"//") {
									x = &x[..x.len() - 2];
								}
								if x.ends_with(b":") {
									x = &x[..x.len() - 1];
								}
								if x.ends_with(b"s") {
									x = &x[..x.len() - 1];
								}
								if x.ends_with(b"http") {
									x = &x[..x.len() - 4];
								}
								target.extend(cfg.prefix_local.as_ref().unwrap().as_bytes());
							} else {
								if x.ends_with(b"https://") {
									x = &x[..x.len() - 8];
									target.extend(b"http://");
								}
								target.extend(cfg.http.rewrite.as_ref().unwrap().as_bytes());
							}
							target.extend(x);
						}
						if method == Method::GET {
							if let Some(parent) = npath.parent() {
								unwrap_void!(create_dir_all(format!("{}/", parent.to_string_lossy())).await);
							}
							unwrap_void!(write(&npath, &target).await);
							if link {
								unwrap_void!(hard_link(&out, &npath));
							}
						}

						break builder.stream_single(target);
					}

					_ => {}
				}
			}

			let file = File::create(&out).await.unwrap();

			if link {
				unwrap_void!(hard_link(&out, &npath));
			}
			let inner = Box::pin(resp.bytes_stream());
			let stream = unfold((file, inner), |(mut file, mut inner)| async move {
				match inner.next().await? {
					Ok(buf) => {
						unwrap_void!(file.write_all(&buf).await);
						Some((Ok(buf), (file, inner)))
					}
					Err(err) => {
						Some((Err(axum::Error::new(err)), (file, inner)))
					}
				}
			});

			break builder.stream(stream);
		}
	}
}