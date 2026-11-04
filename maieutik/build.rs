use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let cpp_dir = manifest_dir.join("src/cpp_modules");

    // Tell cargo to re-run if any C++ file changes
    println!("cargo:rerun-if-changed={}", cpp_dir.display());

    // Compile all C++ modules using cc crate
    cc::Build::new()
        .cpp(true)
        .flag("-std=c++17")
        .flag("-fPIC")
        // file_service
        .file(cpp_dir.join("file_service.cpp"))
        // wiki_links
        .file(cpp_dir.join("wiki_links.cpp"))
        // markdown_tables
        .file(cpp_dir.join("markdown_tables.cpp"))
        // license
        .file(cpp_dir.join("license.cpp"))
        .compile("cpp_modules");

    // Link the compiled static library
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=cpp_modules");
}
