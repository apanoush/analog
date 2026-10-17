//! Link the Clang runtime on macOS.
//!
//! Rust final-links macOS binaries with `-nodefaultlibs`, so Clang's
//! `libclang_rt.osx.a` is not pulled in automatically. The prebuilt ONNX
//! Runtime downloaded by `ort` contains Objective-C++ objects whose
//! `@available` checks call `__isPlatformVersionAtLeast`, a compiler-rt
//! builtin. Link the archive from the active toolchain explicitly.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    if let Some(archive) = clang_runtime_archive() {
        let dir = archive.parent().expect("archive has a parent directory");
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    } else {
        println!(
            "cargo:warning=could not locate libclang_rt.osx.a; \
             ___isPlatformVersionAtLeast may be undefined"
        );
    }
}

/// Return `<clang resource dir>/lib/darwin/libclang_rt.osx.a`.
fn clang_runtime_archive() -> Option<PathBuf> {
    let output = Command::new("clang")
        .arg("-print-resource-dir")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let resource_dir = String::from_utf8(output.stdout).ok()?;
    let archive = Path::new(resource_dir.trim())
        .join("lib")
        .join("darwin")
        .join("libclang_rt.osx.a");
    archive.is_file().then_some(archive)
}
