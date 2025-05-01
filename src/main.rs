use appui::AppUi;
use log::{info, Level};

mod appui;
mod audio_play;
mod decode;
fn main() {
    simple_logger::init_with_level(Level::Warn).unwrap();
    info!("logger init and app banner!!-------------==========");
    let mut tiny_app_ui = appui::AppUi::new();
    tiny_app_ui.init_appui_and_resources();
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "tiny player",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            return Ok(Box::new(tiny_app_ui));
        }),
    )
    .unwrap();
}
