extern crate cc;

fn main() {
    if std::env::var_os("CC").is_none() {
        std::env::set_var("CC", "clang");
    }

    cc::Build::new()
        .file("src/stack.ll")
        .flag("-x")
        .flag("ir")
        .flag("-Wno-override-module")
        .compile("stack");
}
