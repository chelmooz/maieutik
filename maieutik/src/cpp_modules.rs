//! C++ FFI bindings pour fonctions critiques (fichiers, liens wiki, tables, licence)

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

extern "C" {
    // file_service
    fn cpp_list_mcp_files(path: *const c_char, out: *mut *mut *mut c_char) -> c_int;
    fn cpp_read_mcp_file(path: *const c_char) -> *mut c_char;
    fn cpp_free_strings(ptr: *mut *mut c_char, count: c_int);
    fn cpp_free_string(ptr: *mut c_char);

    // wiki_links
    fn cpp_extract_links(content: *const c_char, out: *mut *mut *mut c_char) -> c_int;
    fn cpp_free_links(ptr: *mut *mut c_char, count: c_int);

    // markdown_tables
    fn cpp_create_table(rows: c_int, cols: c_int, headers: *const c_char) -> *mut c_char;
    fn cpp_free_table(ptr: *mut c_char);

    // license
    fn cpp_get_license_status() -> *const c_char;
}

/// Liste les fichiers .md et .txt dans un répertoire (pour MCP)
pub fn list_mcp_files<P: AsRef<std::path::Path>>(path: P) -> Vec<String> {
    let path_str = path.as_ref().to_string_lossy();
    let c_path = CString::new(path_str.as_ref()).unwrap();
    unsafe {
        let mut ptr: *mut *mut c_char = std::ptr::null_mut();
        let count = cpp_list_mcp_files(c_path.as_ptr(), &mut ptr);
        if count <= 0 {
            return Vec::new();
        }
        let mut result = Vec::with_capacity(count as usize);
        for i in 0..count {
            result.push(
                CStr::from_ptr(*ptr.add(i as usize))
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        // Zero-copy version: memory managed statically in C++, no free needed
        result
    }
}

/// Lit le contenu d'un fichier
pub fn read_mcp_file<P: AsRef<std::path::Path>>(path: P) -> Option<String> {
    let path_str = path.as_ref().to_string_lossy();
    let c_path = CString::new(path_str.as_ref()).unwrap();
    unsafe {
        let ptr = cpp_read_mcp_file(c_path.as_ptr());
        if ptr.is_null() {
            return None;
        }
        let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        cpp_free_string(ptr);
        Some(s)
    }
}

/// Extrait les liens wiki [[...]] d'un contenu
pub fn extract_links(content: &str) -> Vec<String> {
    let c_content = CString::new(content).unwrap();
    unsafe {
        let mut ptr: *mut *mut c_char = std::ptr::null_mut();
        let count = cpp_extract_links(c_content.as_ptr(), &mut ptr);
        if count <= 0 {
            return Vec::new();
        }
        let mut result = Vec::with_capacity(count as usize);
        for i in 0..count {
            result.push(
                CStr::from_ptr(*ptr.add(i as usize))
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        cpp_free_links(ptr, count);
        result
    }
}

/// Crée une table Markdown
pub fn create_table(rows: usize, cols: usize, headers: Option<&str>) -> String {
    // Keep CString alive for the duration of the call
    let c_headers_opt = headers.map(|h| CString::new(h).unwrap());
    let headers_ptr = c_headers_opt
        .as_ref()
        .map_or(std::ptr::null(), |h| h.as_ptr());
    unsafe {
        let ptr = cpp_create_table(rows as i32, cols as i32, headers_ptr);
        if ptr.is_null() {
            return String::new();
        }
        let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        cpp_free_table(ptr);
        s
    }
}

/// Récupère le statut de la licence
pub fn get_license_status() -> &'static str {
    unsafe { CStr::from_ptr(cpp_get_license_status()).to_str().unwrap() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_list_mcp_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("test1.md"), "content1").unwrap();
        fs::write(dir.path().join("test2.txt"), "content2").unwrap();
        fs::write(dir.path().join("test3.rs"), "content3").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();

        let files = list_mcp_files(dir.path());
        assert!(!files.is_empty());
        // Should only find .md and .txt files
        for file in &files {
            assert!(file.ends_with(".md") || file.ends_with(".txt"));
        }
        println!("Files: {:?}", files);
    }

    #[test]
    fn test_read_mcp_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.md");
        fs::write(&file_path, "Hello, World!").unwrap();

        let content = read_mcp_file(&file_path);
        assert_eq!(content, Some("Hello, World!".to_string()));
    }

    #[test]
    fn test_extract_links() {
        let content = "See [[link1]] and [[link2]] and [[link3]]";
        let links = extract_links(content);
        assert_eq!(links.len(), 3);
        assert!(links.contains(&"link1".to_string()));
        assert!(links.contains(&"link2".to_string()));
        assert!(links.contains(&"link3".to_string()));
    }

    #[test]
    fn test_create_table() {
        let table = create_table(2, 3, Some("Name|Age|City"));
        println!("Table output:\n{}", table);
        assert!(table.contains("| Name | Age | City |"));
        assert!(table.contains("|---|---|---|"));
        assert!(table.contains("|  |  |  |"));
    }

    #[test]
    fn test_get_license_status() {
        let status = get_license_status();
        assert_eq!(status, "MIT");
    }
}
