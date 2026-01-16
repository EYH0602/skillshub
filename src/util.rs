use anyhow::Result;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Truncate a string to max length with ellipsis
pub fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Recursively copy a directory
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in WalkDir::new(src).min_depth(1) {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(src)?;
        let dest_path = dst.join(relative);

        if path.is_dir() {
            fs::create_dir_all(&dest_path)?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, &dest_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_truncate_string_short() {
        assert_eq!(truncate_string("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_string_exact() {
        assert_eq!(truncate_string("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_string_long() {
        assert_eq!(truncate_string("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_string_empty() {
        assert_eq!(truncate_string("", 5), "");
    }

    #[test]
    fn test_truncate_string_very_short_max() {
        assert_eq!(truncate_string("hello", 3), "...");
    }

    #[test]
    fn test_copy_dir_recursive() {
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();

        // Create source structure
        fs::write(src_dir.path().join("file1.txt"), "content1").unwrap();
        fs::create_dir(src_dir.path().join("subdir")).unwrap();
        fs::write(src_dir.path().join("subdir/file2.txt"), "content2").unwrap();

        let dst_path = dst_dir.path().join("copied");
        copy_dir_recursive(src_dir.path(), &dst_path).unwrap();

        // Verify copied structure
        assert!(dst_path.join("file1.txt").exists());
        assert_eq!(
            fs::read_to_string(dst_path.join("file1.txt")).unwrap(),
            "content1"
        );
        assert!(dst_path.join("subdir/file2.txt").exists());
        assert_eq!(
            fs::read_to_string(dst_path.join("subdir/file2.txt")).unwrap(),
            "content2"
        );
    }
}
