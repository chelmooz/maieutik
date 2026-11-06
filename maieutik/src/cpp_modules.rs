//! C++ FFI bindings pour fonctions critiques (fichiers, liens wiki, tables, licence)

use anyhow::{Context, Result};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::Path;

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
///
/// # Errors
/// Returns an error if the path cannot be converted to a C string.
pub fn list_mcp_files<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    let path_str = path.as_ref().to_string_lossy();
    let c_path = CString::new(path_str.as_ref()).context("Failed to create CString from path")?;
    unsafe {
        let mut ptr: *mut *mut c_char = std::ptr::null_mut();
        let count = cpp_list_mcp_files(c_path.as_ptr(), &mut ptr);
        if count <= 0 {
            return Ok(Vec::new());
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
        Ok(result)
    }
}

/// Lit le contenu d'un fichier
///
/// # Errors
/// Returns an error if the path cannot be converted to a C string.
pub fn read_mcp_file<P: AsRef<Path>>(path: P) -> Result<Option<String>> {
    let path_str = path.as_ref().to_string_lossy();
    let c_path = CString::new(path_str.as_ref()).context("Failed to create CString from path")?;
    unsafe {
        let ptr = cpp_read_mcp_file(c_path.as_ptr());
        if ptr.is_null() {
            return Ok(None);
        }
        let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        cpp_free_string(ptr);
        Ok(Some(s))
    }
}

/// Extrait les liens wiki [[...]] d'un contenu
///
/// # Errors
/// Returns an error if the content cannot be converted to a C string.
pub fn extract_links(content: &str) -> Result<Vec<String>> {
    let c_content = CString::new(content).context("Failed to create CString from content")?;
    unsafe {
        let mut ptr: *mut *mut c_char = std::ptr::null_mut();
        let count = cpp_extract_links(c_content.as_ptr(), &mut ptr);
        if count <= 0 {
            return Ok(Vec::new());
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
        Ok(result)
    }
}

/// Crée une table Markdown
///
/// # Errors
/// Returns an error if the headers cannot be converted to a C string.
pub fn create_table(rows: usize, cols: usize, headers: Option<&str>) -> Result<String> {
    // Keep CString alive for the duration of the call
    let c_headers_opt = headers
        .map(|h| CString::new(h).context("Failed to create CString from headers"))
        .transpose()?;
    let headers_ptr = c_headers_opt
        .as_ref()
        .map_or(std::ptr::null(), |h| h.as_ptr());
    unsafe {
        let ptr = cpp_create_table(rows as i32, cols as i32, headers_ptr);
        if ptr.is_null() {
            return Ok(String::new());
        }
        let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        cpp_free_table(ptr);
        Ok(s)
    }
}

/// Récupère le statut de la licence
///
/// # Panics
/// Panics if the license status from C++ is not valid UTF-8.
/// This is acceptable as it indicates a serious bug in the C++ code.
pub fn get_license_status() -> &'static str {
    unsafe {
        let c_str = CStr::from_ptr(cpp_get_license_status());
        c_str
            .to_str()
            .expect("License status from C++ must be valid UTF-8")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_list_mcp_files() {
        let dir = tempdir().expect("test");
        fs::write(dir.path().join("test1.md"), "content1").expect("test");
        fs::write(dir.path().join("test2.txt"), "content2").expect("test");
        fs::write(dir.path().join("test3.rs"), "content3").expect("test");
        fs::create_dir(dir.path().join("subdir")).expect("test");

        let files = list_mcp_files(dir.path()).expect("list_mcp_files should succeed");
        assert!(!files.is_empty());
        // Should only find .md and .txt files
        for file in &files {
            assert!(file.ends_with(".md") || file.ends_with(".txt"));
        }
        println!("Files: {:?}", files);
    }

    #[test]
    fn test_read_mcp_file() {
        let dir = tempdir().expect("test");
        let file_path = dir.path().join("test.md");
        fs::write(&file_path, "Hello, World!").expect("test");

        let content = read_mcp_file(&file_path).expect("read_mcp_file should succeed");
        assert_eq!(content, Some("Hello, World!".to_string()));
    }

    #[test]
    fn test_extract_links() {
        let content = "See [[link1]] and [[link2]] and [[link3]]";
        let links = extract_links(content).expect("extract_links should succeed");
        assert_eq!(links.len(), 3);
        assert!(links.contains(&"link1".to_string()));
        assert!(links.contains(&"link2".to_string()));
        assert!(links.contains(&"link3".to_string()));
    }

    #[test]
    fn test_create_table() {
        let table = create_table(2, 3, Some("Name|Age|City")).expect("create_table should succeed");
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
