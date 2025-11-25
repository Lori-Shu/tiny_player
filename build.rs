use std::{fs, path::Path};

fn main() {
    let mut dlls = vec![];
    let folder_path = Path::new("D:/program/ffmpeg-n8.0-latest-win64-gpl-shared-8.0/bin");
    if let Ok(dir) = folder_path.read_dir() {
        for dir_entry in dir {
            if let Ok(p) = dir_entry {
                if p.file_name().to_str().unwrap().ends_with(".dll") {
                    dlls.push(p.path());
                }
            }
        }
    }
    for dll in dlls {
        let resource_dest =
            Path::new("resources/dlls").join(dll.file_name().unwrap().to_str().unwrap());
        let target_debug_dest =
            Path::new("target/debug").join(dll.file_name().unwrap().to_str().unwrap());
        fs::copy(&dll, resource_dest).unwrap();
        fs::copy(&dll, target_debug_dest).unwrap();
    }
    if std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("resources/desktop_icon.ico");
        res.compile().unwrap();
    }
}
