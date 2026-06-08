pub mod redis;
pub mod mongo;
pub mod postgres;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type LogCallback = Arc<dyn Fn(String) + Send + Sync + 'static>;

pub trait DatabaseEngine: Send + Sync {
    fn start(
        &self,
        host: String,
        port: u16,
        on_log: LogCallback,
        on_debug: LogCallback,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;
    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;
    fn wipe(&self) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;
    fn execute_raw(&self, query: String) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;
    fn is_running(&self) -> bool;
}
