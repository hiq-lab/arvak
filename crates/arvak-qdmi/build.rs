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

    // Build the mock twice: once with the QDMI 1.2/1.3 ABI and once with
    // the QDMI 1.4 ABI, so tests exercise both loader paths.
    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let variants = [
        ("MOCK_QDMI_DEVICE_PATH", "libmock_qdmi_device", None),
        (
            "MOCK_QDMI_DEVICE_V14_PATH",
            "libmock_qdmi_device_v14",
            Some("-DMOCK_QDMI_V14"),
        ),
    ];

    for (env_var, stem, define) in variants {
        let so_path = out_dir.join(format!("{stem}.{ext}"));

        let mut cmd = Command::new("cc");
        cmd.args([
            "-shared",
            "-fPIC",
            "-o",
            so_path.to_str().unwrap(),
            mock_src,
            "-Wall",
            "-Wextra",
            "-O2",
        ]);
        if let Some(define) = define {
            cmd.arg(define);
        }
        let status = cmd.status().expect("failed to invoke C compiler");

        assert!(
            status.success(),
            "failed to compile mock QDMI device ({stem}): {status}"
        );

        // Tell cargo where to find the compiled mock device.
        println!("cargo:rustc-env={env_var}={}", so_path.display());
    }

    // Re-run if the mock device source changes.
    println!("cargo:rerun-if-changed={mock_src}");
}
