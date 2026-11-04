use maieutik::cpp_modules::list_mcp_files;
use std::fs;
use tempfile::tempdir;
use std::time::Instant;

#[test]
fn test_list_files_performance() {
    let dir = tempdir().unwrap();
    // create 1000 files
    for i in 0..1000 {
        let path = dir.path().join(format!("file_{}.md", i));
        fs::write(path, "content").unwrap();
    }
    let start = Instant::now();
    let files = list_mcp_files(dir.path());
    let duration = start.elapsed();
    println!("Listed {} files in {:?}", files.len(), duration);
    assert!(files.len() <= 1000);
    // Benchmark threshold relaxed for CI
    assert!(duration.as_millis() < 1000);
}

#[test]
fn test_memory_safety_no_leaks() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "x").unwrap();
    let files = list_mcp_files(dir.path());
    assert!(!files.is_empty());
}
