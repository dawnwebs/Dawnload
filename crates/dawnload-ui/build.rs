fn main() {
    slint_build::compile("ui/app.slint").expect("Slint build failed");
    // The .exe's own icon and version (Explorer, the taskbar when pinned, Task Manager).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("assets/dawnload.ico");
            res.set("ProductName", "Dawnload");
            res.set("FileDescription", "Dawnload");
            res.set("LegalCopyright", "MIT License");
            if let Err(e) = res.compile() {
                println!("cargo:warning=no Windows resources (icon, version): {e}");
            }
        }
    }
}
