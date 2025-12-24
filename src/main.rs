#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
#![deny(unused)]
#![deny(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use std::{
    error::Error,
    fmt::Display,
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use eframe::{
    egui_wgpu::{WgpuSetup, WgpuSetupCreateNew},
    wgpu::{Backends, InstanceDescriptor},
};
use egui::{IconData, ImageSource, Vec2, include_image};
use log::{Level, warn};

mod ai_sub_title;
mod appui;
mod async_context;
mod audio_play;
mod decode;

const WINDOW_ICON: ImageSource = include_image!("../resources/play_img.png");
static CURRENT_EXE_PATH: LazyLock<PlayerResult<PathBuf>> = LazyLock::new(|| {
    if let Ok(path) = std::env::current_exe() {
        return Ok(path);
    }
    Err(PlayerError::Internal("can not find exe path!".to_string()))
});
#[derive(Debug, Clone)]
pub enum PlayerError {
    Internal(String),
}
impl Display for PlayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Internal(s) => if let Ok(()) = write!(f, "error: {}", s) {},
        }

        Ok(())
    }
}
impl Error for PlayerError {}
pub type PlayerResult<T> = std::result::Result<T, PlayerError>;

/// main fun init log, init main ui type Appui
fn main() {
    if simple_logger::init_with_level(Level::Warn).is_ok() {
        warn!("logger init success\napp banner!!====================");
    }

    if let Ok(tiny_app_ui) = appui::AppUi::new() {
        let mut options = eframe::NativeOptions::default();
        options.renderer = eframe::Renderer::Wgpu;
        options.wgpu_options.wgpu_setup = WgpuSetup::CreateNew(WgpuSetupCreateNew {
            instance_descriptor: InstanceDescriptor {
                backends: Backends::VULKAN,
                ..Default::default()
            },
            ..Default::default()
        });
        if let ImageSource::Bytes { bytes, .. } = WINDOW_ICON {
            if let Ok(img) = image::load_from_memory(&bytes) {
                options.viewport.icon = Some(Arc::new(IconData {
                    width: img.width(),
                    height: img.height(),
                    rgba: img.as_bytes().to_vec(),
                }));
            }
        }
        options.centered = true;
        options.viewport.inner_size = Some(Vec2::new(900.0, 700.0));

        if eframe::run_native(
            "tiny player",
            options,
            Box::new(|cc| {
                egui_extras::install_image_loaders(&cc.egui_ctx);
                tiny_app_ui.replace_fonts(&cc.egui_ctx);
                Ok(Box::new(tiny_app_ui))
            }),
        )
        .is_ok()
        {
            warn!("eframe start success");
        }
    }
}
