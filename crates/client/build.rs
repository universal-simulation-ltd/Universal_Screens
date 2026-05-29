//! The doom-fish Apple crates (apple-cf, videotoolbox) embed Swift bridges.
//! Their compiled objects (a) force-load the `swiftCompatibility*` back-deployment
//! static libs at link time and (b) reference Swift runtime dylibs (e.g.
//! `libswift_Concurrency.dylib`) via `@rpath` at run time. On a machine with only
//! Command Line Tools (no full Xcode), the upstream build scripts point at an
//! Xcode-toolchain path that doesn't exist, so both steps fail. Wire up the Swift
//! runtime directories that actually exist here. (Same fix as the host crate.)

fn main() {
    #[cfg(target_os = "macos")]
    configure_swift_runtime();
}

#[cfg(target_os = "macos")]
fn configure_swift_runtime() {
    use std::path::Path;
    use std::process::Command;

    // OS-provided Swift runtime — resolves libswift_Concurrency.dylib and the
    // rest of the dynamic runtime at load time on macOS 12+.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    let Ok(output) = Command::new("xcode-select").arg("-p").output() else {
        return;
    };
    let dev_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if dev_dir.is_empty() {
        return;
    }

    // Command Line Tools and full Xcode lay the Swift runtime out differently;
    // use whichever directory actually exists.
    let candidates = [
        format!("{dev_dir}/usr/lib/swift/macosx"),
        format!("{dev_dir}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx"),
    ];
    for path in candidates {
        if Path::new(&path).is_dir() {
            // Link time: find the swiftCompatibility* static archives.
            println!("cargo:rustc-link-search=native={path}");
            // Run time: fall back here for any dylib not served from /usr/lib/swift.
            println!("cargo:rustc-link-arg=-Wl,-rpath,{path}");
        }
    }
}
