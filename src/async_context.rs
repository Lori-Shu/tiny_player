use std::{path::Path, sync::Arc};

use tokio::{
    runtime::{Handle, Runtime},
    sync::RwLock,
};

use crate::{PlayerError, PlayerResult};

#[derive(Debug, Clone)]
pub struct VideoDes {
    pub name: String,
    pub path: String,
}
pub struct AsyncContext {
    async_runtime: Runtime,
}
impl AsyncContext {
    pub fn new() -> PlayerResult<Self> {
        if let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(Self {
                async_runtime: runtime,
            })
        } else {
            Err(PlayerError::Internal(
                "build async runtime error".to_string(),
            ))
        }
    }
    pub fn exec_normal_task<Fu, Ot>(&self, f: Fu) -> Ot
    where
        Fu: Future<Output = Ot>,
    {
        self.async_runtime.block_on(f)
    }
    pub fn runtime_handle(&self) -> Handle {
        self.async_runtime.handle().clone()
    }
    pub fn read_video_folder(&self, path: &Path, video_des: Arc<RwLock<Vec<VideoDes>>>) {
        let mut video_targets = self.exec_normal_task(video_des.write());
        if let Ok(ite) = path.read_dir() {
            for entry in ite {
                if let Ok(en) = entry {
                    if let Ok(t) = en.file_type() {
                        if t.is_file() {
                            if let Some(file_name) = en.file_name().to_str() {
                                if file_name.ends_with(".ts")
                                    || file_name.ends_with(".mp4")
                                    || file_name.ends_with(".mkv")
                                    || file_name.ends_with(".flac")
                                    || file_name.ends_with(".mp3")
                                    || file_name.ends_with(".m4a")
                                    || file_name.ends_with(".wav")
                                {
                                    if let Some(p_str) = path.join(&file_name).to_str() {
                                        video_targets.push(VideoDes {
                                            name: file_name.to_string(),
                                            path: p_str.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
