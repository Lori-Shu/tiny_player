use egui::TextureHandle;
use tokio::runtime::{Handle, Runtime};

use crate::{PlayerError, PlayerResult};

#[derive(Clone)]
pub struct VideoDes {
    pub name: String,
    pub path: String,
    pub texture_handle: TextureHandle,
}
pub struct AsyncContext {
    async_runtime: Runtime,
}
impl AsyncContext {
    pub fn new() -> PlayerResult<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| PlayerError::Internal(e.to_string()))?;

        Ok(Self {
            async_runtime: runtime,
        })
    }
    pub fn exec_normal_task<F, O>(&self, f: F) -> O
    where
        F: Future<Output = O>,
    {
        self.async_runtime.block_on(f)
    }
    pub fn runtime_handle(&self) -> Handle {
        self.async_runtime.handle().clone()
    }
}
