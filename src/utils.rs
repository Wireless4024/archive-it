use std::path::{Component, Path, PathBuf};

use aho_corasick::AhoCorasick;
use bstr::ByteSlice;

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

		//PathBuf::from(path.as_bstr().to_string()).strip_prefix(&current);
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

pub fn read_dir_recursive(path: &Path) -> Vec<(PathBuf, bool)> {
	let mut res = vec![];
	for e in path.read_dir().unwrap().flatten() {
		if let Ok(meta) = e.metadata() {
			if meta.is_dir() {
				res.push((e.path(), true));
				res.extend(read_dir_recursive(&e.path()));
			} else {
				res.push((e.path(), false));
			}
		}
	}
	res
}