#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
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
mod test {
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
    fn result_with_list() -> Result<Vec<i64>, DivisionError> {
        //                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        let numbers = [27, 297, 38502, 81];
        let mut division_results = numbers.into_iter().map(|n| divide(n, 0));
        // Collects to the expected return type. Returns the first error in the
        // division results (if one exists).
        match division_results.find(|n| match n {
            Ok(num) => return false,
            Err(e) => return true,
        }) {
            Some(r) => return Err(r.unwrap_err()),
            None => {
                let ans: Vec<i64> = division_results
                    .map(|n| {
                        return match n {
                            Ok(num) => return num,
                            Err(e) => return -1,
                        };
                    })
                    .collect();
                return Ok(ans);
            }
        }
    }
    #[test]
    fn test_iter() {
        match result_with_list() {
            Err(e) => match e {
                DivisionError::DivideByZero => {
                    println!("error dividby zero");
                }
                _ => {}
            },
            Ok(_) => {}
        }
    }
    #[test]
    fn test_alg() {}
}
