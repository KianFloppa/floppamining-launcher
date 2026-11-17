fn main() {
    // Windows: the exe gets the Floppa icon and version info (shown in Explorer and the taskbar)
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut r = winresource::WindowsResource::new();
        r.set_icon("assets/icon.ico");
        r.set("ProductName", "Floppa Launcher");
        r.set("FileDescription", "Floppa Launcher");
        r.set("CompanyName", "KianFloppa");
        r.set("LegalCopyright", "KianFloppa");
        if std::env::var("HOST").map(|h| !h.contains("windows")).unwrap_or(false) {
            r.set_toolkit_path("/usr/bin");
            r.set_windres_path("x86_64-w64-mingw32-windres");
            r.set_ar_path("x86_64-w64-mingw32-ar");
        }
        r.compile().expect("windows resources");
    }
}
