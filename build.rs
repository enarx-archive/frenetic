extern crate cc;

fn main() {
    if std::env::var_os("CC").is_none() {
        std::env::set_var("CC", "clang");
    }

    cc::Build::new()
        .file("src/jump.ll")
        .flag("-x")
        .flag("ir")
        .flag("-Wno-override-module")
        .compile("stack");

    if probe("#![feature(generator_trait)] fn main() {}") {
        println!("cargo:rustc-cfg=has_generator_trait");
    }
}

/// Test if a code snippet can be compiled
fn probe(code: &str) -> bool {
    use std::process::{Command, Stdio};
    use std::io::Write;
    use std::env;

    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let out_dir = env::var_os("OUT_DIR").expect("environment variable OUT_DIR");

    let mut child = Command::new(rustc)
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--emit=obj")
        .arg("-")
        .stdin(Stdio::piped())
        .spawn()
        .expect("rustc probe");

    child
        .stdin
        .as_mut()
        .expect("rustc stdin")
        .write_all(code.as_bytes())
        .expect("write rustc stdin");

    child.wait().expect("rustc probe").success()
}
