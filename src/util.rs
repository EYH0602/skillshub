use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn truncate_string(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        value.to_string()
    } else {
        format!("{}...", &value[..max_len.saturating_sub(3)])
    }
}

/// Recursively copy directory contents
///
/// Symlinks are skipped as a defense-in-depth measure to prevent a malicious
/// cloned repo from including symlinks that point outside the clone directory.
pub fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;

        // Skip symlinks to avoid following links that escape the source tree
        if entry.file_type()?.is_symlink() {
            continue;
        }

        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            fs::create_dir_all(&dst_path)?;
            copy_dir_contents(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(truncate_string("hello world", 8), "hello...");
    }

    #[test]
    fn test_copy_dir_contents_copies_tree() {
        use tempfile::TempDir;
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        fs::create_dir_all(src.path().join("subdir")).unwrap();
        fs::write(src.path().join("file.txt"), b"hello").unwrap();
        fs::write(src.path().join("subdir/nested.txt"), b"world").unwrap();

        copy_dir_contents(src.path(), dst.path()).unwrap();

        assert!(dst.path().join("file.txt").exists());
        assert!(dst.path().join("subdir/nested.txt").exists());
        assert_eq!(fs::read(dst.path().join("file.txt")).unwrap(), b"hello");
        assert_eq!(fs::read(dst.path().join("subdir/nested.txt")).unwrap(), b"world");
    }

    #[test]
    fn test_copy_dir_contents_handles_empty_dir() {
        use tempfile::TempDir;
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        copy_dir_contents(src.path(), dst.path()).unwrap();

        let entries: Vec<_> = fs::read_dir(dst.path()).unwrap().collect();
        assert!(
            entries.is_empty(),
            "destination should be empty after copying empty source"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_copy_dir_contents_skips_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();

        // Create a regular file
        fs::write(src.join("real.txt"), "real content").unwrap();

        // Create a subdirectory with a file
        let subdir = src.join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("nested.txt"), "nested content").unwrap();

        // Create a symlink to a file outside the source tree
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, "outside content").unwrap();
        symlink(&outside, src.join("link-to-file")).unwrap();

        // Create a symlink to a directory outside the source tree
        let outside_dir = temp.path().join("outside-dir");
        fs::create_dir_all(&outside_dir).unwrap();
        fs::write(outside_dir.join("secret.txt"), "secret").unwrap();
        symlink(&outside_dir, src.join("link-to-dir")).unwrap();

        // Run copy
        copy_dir_contents(&src, &dst).unwrap();

        // Regular file and subdirectory should be copied
        assert!(dst.join("real.txt").exists(), "regular file should be copied");
        assert_eq!(fs::read_to_string(dst.join("real.txt")).unwrap(), "real content");
        assert!(
            dst.join("subdir").join("nested.txt").exists(),
            "nested file should be copied"
        );

        // Symlinks should NOT be copied
        assert!(!dst.join("link-to-file").exists(), "symlink to file should be skipped");
        assert!(
            !dst.join("link-to-dir").exists(),
            "symlink to directory should be skipped"
        );
    }

    /// Verify that the `colored` crate suppresses ANSI escape codes when
    /// the `NO_COLOR` environment variable is set (per <https://no-color.org>).
    ///
    /// The global `SHOULD_COLORIZE` is a lazy_static that reads the environment
    /// once at initialization and caches the result. Setting `NO_COLOR` after
    /// that has no effect on the cached value. To test the env-var logic
    /// without `set_override` (which bypasses the env var entirely), we
    /// construct a fresh `ShouldColorize::from_env()` with `NO_COLOR` set
    /// and verify that `should_colorize()` returns false.
    #[test]
    #[serial]
    fn test_no_color_env_suppresses_ansi_codes() {
        // Save previous env state
        let prev_no_color = std::env::var("NO_COLOR").ok();
        let prev_clicolor_force = std::env::var("CLICOLOR_FORCE").ok();

        // Set NO_COLOR and clear CLICOLOR_FORCE (which would take priority)
        std::env::set_var("NO_COLOR", "1");
        std::env::remove_var("CLICOLOR_FORCE");

        // Build a fresh ShouldColorize from the current environment.
        // This reads NO_COLOR at construction time, unlike the global
        // lazy_static which only reads it once.
        let should = colored::control::ShouldColorize::from_env();
        assert!(
            !should.should_colorize(),
            "ShouldColorize::from_env() should return false when NO_COLOR=1 is set"
        );

        // Restore previous env state
        match prev_no_color {
            Some(v) => std::env::set_var("NO_COLOR", v),
            None => std::env::remove_var("NO_COLOR"),
        }
        match prev_clicolor_force {
            Some(v) => std::env::set_var("CLICOLOR_FORCE", v),
            None => {} // was already absent, leave it absent
        }
    }
}
