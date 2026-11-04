use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let cpp_dir = manifest_dir.join("cpp_modules/file_service/src");
    let lib_path = manifest_dir.join("libfile_service.so");
    
    // Tell cargo to re-run if any C++ file changes
    println!("cargo:rerun-if-changed={}", cpp_dir.display());
    println!("cargo:rerun-if-changed={}/file_service.cpp", cpp_dir.display());
    
    // Compile the C++ code using cc crate
    cc::Build::new()
        .cpp(true)
        .flag("-std=c++17")
        .flag("-fPIC")
        .include("cpp_modules/file_service/include")
        .file(cpp_dir.join("file_service.cpp"))
        .shared_flag(true)
        .out_dir(&out_dir)
        .compile("file_service");
    
    // Link the library
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=dylib=file_service");
    
    // Also copy the .so to project root for easy testing
    let _ = std::fs::copy(
        PathBuf::from(&out_dir).join("libfile_service.so"),
        &lib_path
    );
}
