use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Component, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use aho_corasick::AhoCorasick;
use axum::{debug_handler, Extension, Router};
use axum::body::{Bytes, HttpBody, StreamBody};
use axum::extract::{Path, Query, RawBody};
use axum::http::{HeaderMap, HeaderValue};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE, STRICT_TRANSPORT_SECURITY};
use axum::response::Response;
use axum::routing::get;
use bstr::ByteSlice;
use clap::Parser;
use futures_util::{Stream, StreamExt};
use futures_util::stream::unfold;
use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use reqwest::{Client, Method};
use tokio::fs::{create_dir_all, File, read, write};
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

#[derive(Parser)]
#[command(version, long_about = None)]
struct Config {
	/// secure upstream
	#[arg(short, long, default_value_t = true)]
	secure: bool,
	/// upstream host
	host: String,
	/// output dir
	output: PathBuf,
	/// listen port
	#[arg(short, long, default_value_t = 3000)]
	listen: u16,
	/// provide value to replace upstream host with
	#[arg(short, long, default_value = "Option::None")]
	prefix_local: Option<String>,
	// replace url that start with / to relative path
	//#[arg(short, long)]
	//rewrite_prefix: bool,
}

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt().init();
	//extend_absolute(br#"<a href="/abcd/dsads/dsadsa/abc.js"></a>"#, "".as_ref(), &mut Vec::new());
	let cfg = Config::parse();
	let listen = cfg.listen;
	let app = Router::new()
		.route("/*path", get(get_proxy).fallback(get_proxy))
		.route("/", get(get_root).fallback(get_root))
		.layer(Extension(Arc::new(cfg)));

	let addr = SocketAddr::from(([127, 0, 0, 1], listen));
	info!("Listening at http://localhost:{listen}");
	axum::Server::bind(&addr)
		.serve(app.into_make_service())
		.await
		.unwrap();
}

macro_rules! stream_single {
    ($vec:expr) => {
	    {
		    let stream: Pin<Box<dyn Stream<Item=Result<Bytes, axum::Error>> + Send + Sync>> = Box::pin(
				unfold(
					Some(Bytes::from($vec)),
					|it| async {
						Some((Ok(it?), None))
					},
				)
			);
		    stream
	    }
    };
}

macro_rules! box_stream {
    ($stream:expr) => {
	    {
		    let stream: Pin<Box<dyn Stream<Item=Result<Bytes, axum::Error>> + Send + Sync>> = Box::pin(
				$stream	
			);
		    stream
	    }
    };
}

#[debug_handler]
async fn get_root(header: HeaderMap, extension: Extension<Arc<Config>>, q: Query<HashMap<String, String>>, payload: RawBody)
                  -> Response<StreamBody<Pin<Box<dyn Stream<Item=Result<Bytes, axum::Error>> + Send + Sync>>>> {
	get_proxy(header, Path(String::new()), q, extension, payload).await
}

async fn get_proxy(header: HeaderMap,
                   Path(path): Path<String>,
                   Query(q): Query<HashMap<String, String>>,
                   Extension(cfg): Extension<Arc<Config>>,
                   RawBody(payload): RawBody)
                   -> Response<StreamBody<Pin<Box<dyn Stream<Item=Result<Bytes, axum::Error>> + Send + Sync>>>> {
	let npath = if path.is_empty() {
		cfg.output.join("index.html")
	} else {
		let mut path = cfg.output.join(&path);

		if path.as_os_str().to_string_lossy().ends_with('/') || path.extension().is_none() {
			if let Some(name) = path.file_name() {
				let mut last = name.to_os_string();
				last.push(".html");
				path.pop();
				path.push(last);
			}
		}
		if !q.is_empty() && cfg.prefix_local.is_none() {
			let name = if let Some(name) = path.file_stem() {
				name
			} else {
				path.file_name().unwrap()
			};
			let mut last = name.to_os_string();
			let ext = path.extension();
			for (k, v) in q.iter() {
				last.push("-");
				last.push(percent_encode(k.as_bytes(), NON_ALPHANUMERIC).to_string());
				last.push("=");
				last.push(percent_encode(v.as_bytes(), NON_ALPHANUMERIC).to_string());
			}
			if let Some(ext) = ext {
				last.push(".");
				last.push(ext);
			}
			path.pop();
			path.push(last);
		}
		path
	};

	if let Some(parent) = npath.parent() {
		if let Err(e) = create_dir_all(parent).await {
			warn!("{e} at {parent:?}");
		}
	}
	let method = Method::from_bytes(header.get("method").unwrap_or(&HeaderValue::from_static("GET")).as_bytes()).unwrap_or_default();
	let mut builder = Response::builder();
	if let (true, Ok(target)) = (method == Method::GET, read(&npath).await) {
		info!("Serving:    {path:?}");
		let typ = mime_guess::from_path(&npath);
		builder = builder.header(CONTENT_TYPE, typ.first().unwrap_or(mime_guess::mime::TEXT_HTML).to_string());

		builder.body(StreamBody::new(stream_single!(target))).unwrap()
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
		loop {
			let mut builder = Response::builder();
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
						//let is_html = ct.contains_str(b"html");
						let body: Bytes = resp.bytes().await.unwrap_or_default();
						let mut target = Vec::with_capacity(body.len());
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
								target.extend(format!("localhost:{}", cfg.listen).as_bytes());
							}
							target.extend(x);
						}
						if method == Method::GET {
							if let Some(parent) = npath.parent() {
								if let Err(e) = create_dir_all(format!("{}/", parent.to_string_lossy())).await {
									warn!("{e} at {parent:?}");
								};
							}
							if let Err(e) = write(&npath, &target).await {
								warn!("{e} at {npath:?}");
							};
						}

						break builder.body(StreamBody::new(stream_single!(target))).unwrap();
					}

					_ => {}
				}
			}

			let file = File::create(npath).await.unwrap();
			let inner = Box::pin(resp.bytes_stream());
			let stream = unfold((file, inner), |(mut file, mut inner)| async move {
				match inner.next().await? {
					Ok(buf) => {
						if let Err(e) = file.write_all(&buf).await {
							warn!("Error during write file: {e}");
						}
						Some((Ok(buf), (file, inner)))
					}
					Err(err) => {
						Some((Err(axum::Error::new(err)), (file, inner)))
					}
				}
			});

			break builder.body(StreamBody::new(box_stream!(stream))).unwrap();
		}
	}
}


fn extend_absolute(buf: &[u8], rpath: &[u8], out: &mut Vec<u8>) {
	/*let referrer = header.get(REFERER)
		.map(|it| {
			let mut b = it.as_bytes();
			let mut pos = 1;
			let end = b.len() - 1;
			if b[pos - 1] == b'/' && b[pos] != b'/' {
				//b = &b[1..];
			} else {
				while pos < end {
					if b[pos - 1] != b'/' && b[pos] == b'/' && b[pos + 1] != b'/' {
						b = &b[pos ..];
						break;
					}
					pos += 1;
				}
			}
			b
		})
		.unwrap_or_default();*/

	//extend_absolute(x, referrer, &mut target);

	let searcher = AhoCorasick::new(["href=\"/", "src=\"/"]);
	let mut off = 0;
	let path = PathBuf::from(rpath.as_bstr().to_string());
	let current = if rpath.ends_with(b"/") {
		path
	} else {
		path.parent().unwrap_or(&path).to_path_buf()
	};
	let len = buf.len();
	for x in searcher.find_iter(buf) {
		let start = x.end();
		out.extend(&buf[off..start - 1]);
		let mut end = start;
		while end < len {
			if buf[end] == b'"' { break; }
			end += 1;
		}
		let path = &buf[start..end];
		let mut res_pfx = vec![Component::CurDir];

		PathBuf::from(path.as_bstr().to_string()).strip_prefix(&current);
		out.extend(PathBuf::from_iter(res_pfx).to_string_lossy().as_bytes());
		off = end;
	}
	out.extend(&buf[off..]);
	/*if let Some(head) = chunks.next() {
		out.extend(head);
	}
	for x in chunks {
		out.extend(x);
	}*/
	// "href=\"/"
}