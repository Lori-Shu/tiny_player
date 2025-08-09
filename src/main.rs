#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
#![deny(unused_variables)]
use log::{Level, info};

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
            tiny_app_ui.replace_fonts(&cc.egui_ctx);
            return Ok(Box::new(tiny_app_ui));
        }),
    )
    .unwrap();
}
#[cfg(test)]
mod test {
    #[derive(Debug)]
    enum DivisionError {
        // Example: 42 / 0
        DivideByZero,
        // Only case for `i64`: `i64::MIN / -1` because the result is `i64::MAX + 1`
        IntegerOverflow,
        // Example: 5 / 2 = 2.5
        NotDivisible,
    }
    fn divide(a: i64, b: i64) -> Result<i64, DivisionError> {
        if b == 0 {
            return Err(DivisionError::DivideByZero);
        }

        if a == i64::MIN && b == -1 {
            return Err(DivisionError::IntegerOverflow);
        }

        if a % b != 0 {
            return Err(DivisionError::NotDivisible);
        }

        Ok(a / b)
    }
    #[test]
    fn testdivide() {
        let divide = divide(20, 0).unwrap();
        println!("{:#?}", divide);
    }
}
