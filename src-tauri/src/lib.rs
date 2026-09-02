pub mod commands;
pub mod dto;

use colui_app::{
    IdGenerator, LifecycleRuntime, ProfileStore, RuntimeConnector, RuntimeStateReader,
};
use std::sync::Arc;

pub trait RuntimePort: RuntimeConnector + RuntimeStateReader + LifecycleRuntime {}
impl<T> RuntimePort for T where T: RuntimeConnector + RuntimeStateReader + LifecycleRuntime {}

pub struct AppState {
    pub profiles: Arc<dyn ProfileStore>,
    pub ids: Arc<dyn IdGenerator>,
    pub runtime: Arc<dyn RuntimePort>,
}

pub fn run() {
    // Runtime bootstrap is supplied by later application wiring.
}
