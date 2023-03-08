use std::future::Future;

pub enum MaybeAsync<R, F: Future<Output=R>> {
	Sync(R),
	Async(F),
}

#[macro_export]
macro_rules! maybe_await {
    ($maybe_async:expr) => {
	    match ms {
			MaybeAsync::Sync(r) => r,
			MaybeAsync::Async(f)=> f.await,
		}
    };
}
