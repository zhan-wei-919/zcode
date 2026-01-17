use std::future::Future;
use std::pin::Pin;

pub type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub trait AsyncExecutor: Send + Sync {
    fn spawn(&self, task: BoxFuture);
}
