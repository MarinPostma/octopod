use futures::{future::BoxFuture, Future};

pub use inventory;

use crate::App;

inventory::collect!(TestDecl);

#[doc(hidden)]
pub trait TestFn: Send + Sync {
    fn call(&self, app: App) -> BoxFuture<()>;
}

impl<F, Fut> TestFn for F
where
    F: Fn(App) -> Fut + Send + Sync,
    Fut: Future<Output = ()> + Send + Sync + 'static,
{
    fn call(&self, app: App) -> BoxFuture<()> {
        Box::pin(self(app))
    }
}

#[doc(hidden)]
pub struct TestDecl {
    pub name: &'static str,
    pub target_apps: &'static [&'static str],
    pub f: &'static dyn TestFn,
}
