extern crate bindgen;
extern crate cmake;

use std::env;
use std::path::PathBuf;

fn main() {
    let dst = cmake::Config::new("src/openal-soft")
        .define("LIBTYPE", "STATIC")
        .build();

    let mut lib_search_path = dst.clone();
    lib_search_path.push("lib");

    let mut include_path = dst;
    include_path.push("include");
    include_path.push("AL");

    println!(
        "cargo:rustc-link-search=native={}",
        lib_search_path.display()
    );
    println!("cargo:rustc-link-lib=static=openal");
    println!("cargo:rustc-link-lib=dylib=stdc++");

    let mut al_header: PathBuf = include_path.clone();
    al_header.push("al.h");
    let mut alc_header = include_path.clone();
    alc_header.push("alc.h");

    let bindings = bindgen::builder()
        .header(al_header.to_string_lossy())
        .header(alc_header.to_string_lossy())
        .clang_arg("-DAL_LIBTYPE_STATIC")
        .prepend_enum_name(false)
        .generate()
        .unwrap()
        .to_string();

    let mut al_bindings_path = env::var_os("OUT_DIR").unwrap();
    al_bindings_path.push("/openal.rs");

    std::fs::write(al_bindings_path, bindings.as_bytes()).unwrap();
}
