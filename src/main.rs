use appui::AppUi;

mod decode;
mod audio_play;
mod appui;
fn main() {
    println!("Hello, world!");
    let mut tiny_app_ui=appui::AppUi::new();
    tiny_app_ui.init_appui_and_resources();
    let options=eframe::NativeOptions::default();
//     println!("shader_version==={}",options.shader_version.unwrap().clone().version_declaration());
    eframe::run_native("tiny player", options,Box::new(|cc|{
        egui_extras::install_image_loaders(&cc.egui_ctx);
        return Ok(Box::new(tiny_app_ui));
    })).unwrap();
}
