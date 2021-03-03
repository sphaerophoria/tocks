extern crate bindgen;
extern crate cmake;

use std::env;
use std::path::PathBuf;

fn main() {
    let dst = cmake::build("src/c-toxcore");

    let mut lib_search_path = dst.clone();
    lib_search_path.push("lib");

    let mut include_path = dst;
    include_path.push("include");
    include_path.push("tox");

    println!(
        "cargo:rustc-link-search=native={}",
        lib_search_path.display()
    );
    println!("cargo:rustc-link-lib=static=toxcore");

    let mut toxcore_header: PathBuf = include_path;
    toxcore_header.push("tox.h");

    let bindings = bindgen::builder()
        .header(toxcore_header.to_string_lossy())
        .layout_tests(false)
        .prepend_enum_name(false)
        .generate()
        .unwrap()
        .to_string();

    let mut toxcore_bindings_path = env::var_os("OUT_DIR").unwrap();
    toxcore_bindings_path.push("/toxcore.rs");

    std::fs::write(toxcore_bindings_path, bindings.as_bytes()).unwrap();
}
