fn main() {
    println!("cargo:rerun-if-changed=assets/icons/wisp.ico");

    // Give the Windows executable its icon and version details.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/wisp.ico");
        res.set("ProductName", "Wisp");
        res.set("FileDescription", "Wisp");
        res.compile().expect("failed to embed Windows resources");
    }
}
