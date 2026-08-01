//! Compiles ST's VL53L5CX Ultra Lite Driver and generates Rust bindings for it.
//!
//! The ULD is C and stays C — see `vendor/vl53l5cx/`. Rewriting a vendor
//! ranging driver in Rust would be weeks of work whose only product is a fresh
//! set of bugs in code that already works. What we do write by hand is the
//! platform layer (`src/vl53l5cx/platform.rs`), because that is the part that
//! is specific to this board and is where a port actually goes wrong.

use std::env;
use std::path::PathBuf;

/// Platform functions are implemented on the Rust side with `#[no_mangle]`.
/// bindgen must not also emit `extern` declarations for them, or the same
/// symbol arrives from two directions.
const PLATFORM_FNS: &[&str] = &[
    "VL53L5CX_RdByte",
    "VL53L5CX_WrByte",
    "VL53L5CX_RdMulti",
    "VL53L5CX_WrMulti",
    "VL53L5CX_Reset_Sensor",
    "VL53L5CX_SwapBuffer",
    "VL53L5CX_WaitMs",
];

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor = manifest.join("vendor/vl53l5cx");
    let inc = vendor.join("inc");
    let platform = vendor.join("platform");
    let src = vendor.join("src");

    let sources = [
        "vl53l5cx_api.c",
        "vl53l5cx_plugin_detection_thresholds.c",
        "vl53l5cx_plugin_motion_indicator.c",
        "vl53l5cx_plugin_xtalk.c",
    ];

    // ---- compile the vendor C -------------------------------------------
    //
    // The plugins are compiled but not yet used. Function and data sections
    // let the linker discard whatever we never call, so carrying them costs
    // nothing until the day motion indication or xtalk calibration is wanted.
    let mut build = cc::Build::new();
    build
        .include(&inc)
        .include(&platform)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        // ST's ULD compiles with warnings we are not going to fix in vendor
        // code; silencing them keeps our own warnings visible.
        .warnings(false)
        .opt_level_str("s");

    for f in sources {
        build.file(src.join(f));
    }
    build.compile("vl53l5cx_uld");

    // ---- generate bindings ----------------------------------------------
    let target = env::var("TARGET").unwrap();
    let bindings = bindgen::Builder::default()
        .header(inc.join("vl53l5cx_api.h").to_str().unwrap())
        .clang_arg(format!("--target={target}"))
        .clang_arg(format!("-I{}", inc.display()))
        .clang_arg(format!("-I{}", platform.display()))
        .clang_arg("-ffreestanding")
        // no_std: no libstd types, and core::ffi for the C scalar aliases.
        .use_core()
        .ctypes_prefix("core::ffi")
        // Layout tests need std and a test harness; neither exists here.
        .layout_tests(false)
        .derive_debug(false)
        .blocklist_function(PLATFORM_FNS.join("|"))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("failed to generate VL53L5CX bindings");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out.join("vl53l5cx_bindings.rs"))
        .expect("failed to write VL53L5CX bindings");

    // ---- rebuild triggers ------------------------------------------------
    println!("cargo:rerun-if-changed={}", platform.join("platform.h").display());
    for f in sources {
        println!("cargo:rerun-if-changed={}", src.join(f).display());
    }
}
