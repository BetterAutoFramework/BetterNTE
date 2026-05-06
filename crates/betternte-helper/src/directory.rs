//! Directory utilities

use std::fs;
use std::path::{Path, PathBuf};

/// Delete directory if it exists
pub fn delete_directory(path: &Path, recursive: bool) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    if recursive {
        fs::remove_dir_all(path)
    } else {
        fs::remove_dir(path)
    }
}

/// Delete directory with retry on read-only files
pub fn delete_directory_with_retry(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    delete_readonly_directory_impl(path)
}

#[cfg(windows)]
fn delete_readonly_directory_impl(path: &Path) -> std::io::Result<()> {
    // On Windows, we need to handle read-only files specially
    if !path.exists() {
        return Ok(());
    }

    // First, try to recursively set all files to not read-only
    if path.is_dir() {
        for entry in walkdir(path)? {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    // On Windows, use attrib command to remove read-only
                    let _ = std::process::Command::new("attrib")
                        .args(["-R", &entry.path().to_string_lossy()])
                        .output();
                }
            }
        }
    } else if path.is_file() {
        let _ = std::process::Command::new("attrib")
            .args(["-R", &path.to_string_lossy()])
            .output();
    }

    // Now delete
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(not(windows))]
fn delete_readonly_directory_impl(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn walkdir(path: &Path) -> std::io::Result<Vec<fs::DirEntry>> {
    let mut entries = Vec::new();
    walkdir_recursive(path, &mut entries)?;
    Ok(entries)
}

fn walkdir_recursive(path: &Path, entries: &mut Vec<fs::DirEntry>) -> std::io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walkdir_recursive(&path, entries)?;
            } else {
                entries.push(entry);
            }
        }
    }
    Ok(())
}

/// Copy directory recursively
pub fn copy_directory(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !src.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Source is not a directory",
        ));
    }

    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_directory(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Get directory size in bytes
pub fn directory_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;

    if path.is_file() {
        return Ok(path.metadata()?.len());
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            total += directory_size(&path)?;
        } else {
            total += entry.metadata()?.len();
        }
    }

    Ok(total)
}

/// Create directory if it doesn't exist
pub fn ensure_directory(path: &Path) -> std::io::Result<PathBuf> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(path.to_path_buf())
}

/// Check if directory is empty
pub fn is_directory_empty(path: &Path) -> std::io::Result<bool> {
    if !path.is_dir() {
        return Ok(true);
    }
    let mut entries = fs::read_dir(path)?;
    Ok(entries.next().is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_ensure_directory() {
        let temp_dir = env::temp_dir().join("test_betternte_helper");
        let result = ensure_directory(&temp_dir);
        assert!(result.is_ok());
        assert!(temp_dir.exists());
        let _ = delete_directory(&temp_dir, true);
    }

    #[test]
    fn test_delete_directory_nonexistent() {
        let result = delete_directory(Path::new("/nonexistent/dir"), true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_directory_recursive() {
        let temp_dir = env::temp_dir().join("test_betternte_delete_recursive");
        let sub = temp_dir.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("file.txt"), "hello").unwrap();
        assert!(delete_directory(&temp_dir, true).is_ok());
        assert!(!temp_dir.exists());
    }

    #[test]
    fn test_delete_directory_non_recursive_empty() {
        let temp_dir = env::temp_dir().join("test_betternte_delete_empty");
        fs::create_dir_all(&temp_dir).unwrap();
        assert!(delete_directory(&temp_dir, false).is_ok());
        assert!(!temp_dir.exists());
    }

    #[test]
    fn test_delete_directory_non_recursive_non_empty() {
        let temp_dir = env::temp_dir().join("test_betternte_delete_nonempty");
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(temp_dir.join("file.txt"), "hello").unwrap();
        // Non-recursive delete of non-empty dir should fail
        assert!(delete_directory(&temp_dir, false).is_err());
        let _ = delete_directory(&temp_dir, true);
    }

    #[test]
    fn test_copy_directory() {
        let src = env::temp_dir().join("test_betternte_copy_src");
        let dst = env::temp_dir().join("test_betternte_copy_dst");
        let _ = delete_directory(&src, true);
        let _ = delete_directory(&dst, true);

        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("file1.txt"), "hello").unwrap();
        fs::write(src.join("sub").join("file2.txt"), "world").unwrap();

        assert!(copy_directory(&src, &dst).is_ok());
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("sub").join("file2.txt").exists());
        assert_eq!(fs::read_to_string(dst.join("file1.txt")).unwrap(), "hello");

        let _ = delete_directory(&src, true);
        let _ = delete_directory(&dst, true);
    }

    #[test]
    fn test_copy_directory_not_a_dir() {
        let temp_dir = env::temp_dir().join("test_betternte_copy_notdir");
        let file = temp_dir.join("file.txt");
        let _ = fs::create_dir_all(&temp_dir);
        fs::write(&file, "hello").unwrap();
        let result = copy_directory(&file, &temp_dir.join("dst"));
        assert!(result.is_err());
        let _ = delete_directory(&temp_dir, true);
    }

    #[test]
    fn test_directory_size() {
        let temp_dir = env::temp_dir().join("test_betternte_size");
        let _ = fs::create_dir_all(&temp_dir);
        fs::write(temp_dir.join("a.txt"), "12345").unwrap();
        fs::write(temp_dir.join("b.txt"), "12345").unwrap();
        let size = directory_size(&temp_dir).unwrap();
        assert_eq!(size, 10);
        let _ = delete_directory(&temp_dir, true);
    }

    #[test]
    fn test_is_directory_empty_true() {
        let temp_dir = env::temp_dir().join("test_betternte_empty");
        let _ = fs::create_dir_all(&temp_dir);
        assert!(is_directory_empty(&temp_dir).unwrap());
        let _ = delete_directory(&temp_dir, true);
    }

    #[test]
    fn test_is_directory_empty_false() {
        let temp_dir = env::temp_dir().join("test_betternte_notempty");
        let _ = fs::create_dir_all(&temp_dir);
        fs::write(temp_dir.join("file.txt"), "hello").unwrap();
        assert!(!is_directory_empty(&temp_dir).unwrap());
        let _ = delete_directory(&temp_dir, true);
    }

    #[test]
    fn test_is_directory_empty_not_dir() {
        assert!(is_directory_empty(Path::new("/nonexistent")).unwrap());
    }
}
