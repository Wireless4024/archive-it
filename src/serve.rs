use std::collections::HashMap;
use std::sync::Arc;

use axum::Extension;
use axum::extract::{Path, Query, RawBody};
use axum::http::HeaderMap;
use axum::response::Response;
use tokio::fs::create_dir_all;
use tracing::warn;

use crate::{Config, http_all};
use crate::common::{normalize_url_path, serve_file, StreamResponse};

pub(crate) async fn serve_dir(config: Config) {
	let Config::Serve { path, http } = config else { unreachable!() };

	http_all!(http.listen, get_proxy, get_root, ServeConfig{path,rewrite:http.rewrite.unwrap()});
}

struct ServeConfig {
	path: String,
	rewrite: String,
}

async fn get_root(header: HeaderMap,
                  extension: Extension<Arc<ServeConfig>>,
                  q: Query<HashMap<String, String>>,
                  payload: RawBody) -> StreamResponse {
	get_proxy(header, Path(String::new()), q, extension, payload).await
}

async fn get_proxy(_: HeaderMap, // maybe use later
                   Path(path): Path<String>,
                   Query(q): Query<HashMap<String, String>>,
                   Extension(cfg): Extension<Arc<ServeConfig>>,
                   RawBody(_): RawBody) -> StreamResponse {
	let npath = normalize_url_path(cfg.path.as_ref(), &q, &path, true);

	if let Some(parent) = npath.parent() {
		if let Err(e) = create_dir_all(parent).await {
			warn!("{e} at {parent:?}");
		}
	}
	serve_file(npath, Response::builder()).await
}