use std::{fs, path::Path};

use fs_extra::dir::CopyOptions;

fn main() {
    let mut dlls = vec![];
    let folder_path = Path::new("./resources/dlls");
    if let Ok(dir) = folder_path.read_dir() {
        for p in dir.flatten() {
            if p.file_name().to_str().unwrap().ends_with(".dll") {
                dlls.push(p.path());
            }
        }
    }
    for dll in dlls {
        let target_debug_dest =
            Path::new("target/debug").join(dll.file_name().unwrap().to_str().unwrap());
        fs::copy(&dll, target_debug_dest).unwrap();
    }
    fs_extra::copy_items(
        &[Path::new("./resources/model")],
        Path::new("./target/debug"),
        &CopyOptions::new().overwrite(true),
    )
    .unwrap();
    if std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("resources/desktop_icon.ico");
        res.compile().unwrap();
    }
    let vosk_lib_path = "./resources";
    println!("cargo:rustc-link-search=native={}", vosk_lib_path);
}
