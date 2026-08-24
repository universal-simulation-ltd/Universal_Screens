//! Stamp the Windows executable with the app icon and version metadata.
//!
//! Without this the built `.exe` carries Rust's generic icon, so Explorer, the
//! taskbar and Alt-Tab all show a blank page for an app that has perfectly good
//! artwork — and Add/Remove Programs has no publisher to show. The icon comes
//! from `assets/app-icon.ico`; regenerate it with `python scripts/make-win-ico.py`.
//!
//! Embedding needs a resource compiler (`rc.exe` from the Windows SDK, or
//! `llvm-rc`). If one isn't on the box we warn and carry on rather than failing
//! the build — a working binary with the wrong icon beats no binary at all.

fn main() {
    println!("cargo:rerun-if-changed=assets/app-icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/app-icon.ico");
    res.set("ProductName", "Universal Screens");
    res.set("FileDescription", "Universal Screens — host");
    res.set("CompanyName", "Universal Simulation Ltd");
    res.set("LegalCopyright", "Universal Simulation Ltd — MIT licensed");
    if let Err(e) = res.compile() {
        println!("cargo:warning=couldn't embed the Windows icon/version resource: {e}");
    }
}
