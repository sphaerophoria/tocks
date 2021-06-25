extern crate cpp_build;

fn main() {
    let qt_include_path = std::env::var("DEP_QT_INCLUDE_PATH").unwrap();
    let qt_library_path = std::env::var("DEP_QT_LIBRARY_PATH").unwrap();

    #[allow(unused_mut)]
    let mut config = cpp_build::Config::new();

    if cfg!(target_os = "macos") {
        config.flag("-F");
        config.flag(&qt_library_path);
    }

    config
        .include(&qt_include_path)
        .build("src/lib.rs");
}
