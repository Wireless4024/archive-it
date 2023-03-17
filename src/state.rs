use std::collections::HashMap;

use hyper::Method;

pub struct HttpState {
	pub query: HashMap<String, String>,
	pub method: Method,
}