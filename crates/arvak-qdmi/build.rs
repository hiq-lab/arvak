// SPDX-License-Identifier: Apache-2.0
//! Build script: compile the mock QDMI device shared library for integration tests.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mock_src = "examples/mock_device/mock_device.c";

    // Only build the mock device if the source exists (it's part of this crate).
    if !std::path::Path::new(mock_src).exists() {
        return;
    }

    let lib_name = if cfg!(target_os = "macos") {
        "libmock_qdmi_device.dylib"
    } else {
        "libmock_qdmi_device.so"
    };
    let so_path = out_dir.join(lib_name);

    let status = Command::new("cc")
        .args([
            "-shared",
            "-fPIC",
            "-o",
            so_path.to_str().unwrap(),
            mock_src,
            "-Wall",
            "-Wextra",
            "-O2",
        ])
        .status()
        .expect("failed to invoke C compiler");

    assert!(
        status.success(),
        "failed to compile mock QDMI device: {status}"
    );

    // Tell cargo where to find the compiled mock device.
    println!(
        "cargo:rustc-env=MOCK_QDMI_DEVICE_PATH={}",
        so_path.display()
    );

    // Re-run if the mock device source changes.
    println!("cargo:rerun-if-changed={mock_src}");
}
