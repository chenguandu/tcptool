fn main() {
    // Embed Windows icon resource
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("logo.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=winresource compile failed: {}", e);
        }
    }
}
