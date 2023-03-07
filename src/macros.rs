#[macro_export]
macro_rules! stream_single {
    ($vec:expr) => {
	    {
		    let stream: Pin<Box<dyn Stream<Item=Result<Bytes, axum::Error>> + Send + Sync>> = Box::pin(
				futures_util::stream::unfold(
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

#[macro_export]
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

#[macro_export]
macro_rules! http {
    ($port:expr, $route:expr) => {
	    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], $port));
		tracing::info!("Listening at http://localhost:{}", $port);
		axum::Server::bind(&addr)
			.serve($route.into_make_service())
			.await
			.unwrap();
    };
}

#[macro_export]
macro_rules! http_all {
	($port:expr, $intercept:expr) => {
		$crate::http_all!($port,$intercept,());
	};
	($port:expr, $intercept:expr, $root_intercept:expr) => {
		$crate::http_all!($port,$intercept,$intercept,());
	};
    ($port:expr, $intercept:expr, $root_intercept:expr, $data:expr) => {
	    let app = axum::Router::new()
			.route("/*path", axum::routing::get($intercept).fallback($intercept))
			.route("/", axum::routing::get($root_intercept).fallback($root_intercept))
			.layer(axum::Extension(std::sync::Arc::new($data)));
	    $crate::http!($port, app);
    };
}