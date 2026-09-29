use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=native/eventkit.m");
    println!("cargo:rerun-if-changed=native/Info.plist");
    if env::var("CARGO_CFG_TARGET_OS").unwrap() != "macos" {
        panic!("My Schedule requires macOS and EventKit");
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let object = out.join("eventkit.o");
    let arch = if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "aarch64" {
        "arm64"
    } else {
        "x86_64"
    };
    assert!(Command::new("/usr/bin/clang")
        .args([
            "-fobjc-arc",
            "-fblocks",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-mmacosx-version-min=14.0",
            "-arch",
            arch,
            "-c",
            "native/eventkit.m",
            "-o"
        ])
        .arg(&object)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("/usr/bin/ar")
        .arg("rcs")
        .arg(out.join("libschedule_eventkit.a"))
        .arg(object)
        .status()
        .unwrap()
        .success());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=schedule_eventkit");
    for framework in ["Foundation", "EventKit", "AppKit"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!(
        "cargo:rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        root.join("native/Info.plist").display()
    );
}
