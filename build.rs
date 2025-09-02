use std::{fs, path::Path};

fn main() {
    let dlls = vec![
        "D:/cppproject/vcpkg-2025.03.19/installed/x64-windows/bin/avcodec-61.dll",
        "D:/cppproject/vcpkg-2025.03.19/installed/x64-windows/bin/avdevice-61.dll",
        "D:/cppproject/vcpkg-2025.03.19/installed/x64-windows/bin/avfilter-10.dll",
        "D:/cppproject/vcpkg-2025.03.19/installed/x64-windows/bin/avformat-61.dll",
        "D:/cppproject/vcpkg-2025.03.19/installed/x64-windows/bin/avutil-59.dll",
        "D:/cppproject/vcpkg-2025.03.19/installed/x64-windows/bin/swresample-5.dll",
        "D:/cppproject/vcpkg-2025.03.19/installed/x64-windows/bin/swscale-8.dll",
    ];
    let out_dir = "resources/dlls";
    for dll in dlls {
        let resource_dest = Path::new(out_dir).join(Path::new(dll).file_name().unwrap());
        let target_debug_dest = Path::new("target/debug").join(Path::new(dll).file_name().unwrap());
        fs::copy(dll, resource_dest).unwrap();
        fs::copy(dll, target_debug_dest).unwrap();
    }
    if std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("resources/desktop_icon.ico");
        res.compile().unwrap();
    }
}
