use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/ime-reborn.ico");
    println!("cargo:rerun-if-changed=assets/ime-reborn.rc");
    println!("cargo:rerun-if-changed=assets/ime-reborn.manifest");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"))
        .join("ime-reborn-res.o");
    match Command::new("windres")
        .args(["-i", "assets/ime-reborn.rc", "-o"])
        .arg(&output)
        .status()
    {
        Ok(status) if status.success() => println!("cargo:rustc-link-arg={}", output.display()),
        Ok(status) => println!("cargo:warning=windres exited with {status}; building without icon"),
        Err(error) => {
            println!("cargo:warning=windres unavailable ({error}); building without icon")
        }
    }
}
