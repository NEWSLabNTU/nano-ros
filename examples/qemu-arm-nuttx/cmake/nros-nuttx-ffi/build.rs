use std::env;
use std::path::PathBuf;

fn main() {
    // APP_MAIN_CPP: path to the C++ source file to compile (set by CMake)
    // APP_INCLUDE_DIRS: semicolon-separated include directories (set by CMake)
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let nros_root = manifest_dir.join("../../../..");

    let main_cpp = env::var("APP_MAIN_CPP").unwrap_or_else(|_| {
        panic!(
            "APP_MAIN_CPP not set. Set it to the path of the C++ main source file.\n\
             Example: APP_MAIN_CPP=examples/qemu-arm-nuttx/cpp/zenoh/talker/src/main.cpp"
        )
    });

    let nros_cpp_include = nros_root.join("packages/core/nros-cpp/include");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .file(&main_cpp)
        .include(&nros_cpp_include)
        .flag("-std=c++14")
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .warnings(false);

    // Additional include directories from CMake (semicolon-separated)
    if let Ok(include_dirs) = env::var("APP_INCLUDE_DIRS") {
        for dir in include_dirs.split(';') {
            if !dir.is_empty() {
                build.include(dir);
            }
        }
    }

    build.compile("app");

    println!("cargo:rerun-if-changed={}", main_cpp);
    println!("cargo:rerun-if-env-changed=APP_MAIN_CPP");
    println!("cargo:rerun-if-env-changed=APP_INCLUDE_DIRS");
}
