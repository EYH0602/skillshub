use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Get home directory - supports test override via SKILLSHUB_TEST_HOME env var
pub fn get_home_dir() -> Option<PathBuf> {
    std::env::var("SKILLSHUB_TEST_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
}

/// Get the skillshub home directory (~/.skillshub)
pub fn get_skillshub_home() -> Result<PathBuf> {
    let home = get_home_dir().context("Could not determine home directory")?;
    Ok(home.join(".skillshub"))
}

/// Get the skills installation directory (~/.skillshub/skills)
pub fn get_skills_install_dir() -> Result<PathBuf> {
    Ok(get_skillshub_home()?.join("skills"))
}

/// Get the taps clone directory (~/.skillshub/taps)
pub fn get_taps_clone_dir() -> Result<PathBuf> {
    Ok(get_skillshub_home()?.join("taps"))
}

/// Get the clone directory for a specific tap (~/.skillshub/taps/owner/repo)
#[allow(dead_code)]
pub fn get_tap_clone_dir(tap_name: &str) -> Result<PathBuf> {
    let taps_dir = get_taps_clone_dir()?;
    Ok(crate::registry::git::tap_clone_path(&taps_dir, tap_name))
}

/// Check if a directory looks like a valid skillshub skills directory
/// (contains at least one subdirectory with a SKILL.md file)
fn is_valid_skills_dir(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let skill_md = entry.path().join("SKILL.md");
            if skill_md.exists() {
                return true;
            }
        }
    }
    false
}

/// Get the embedded skills directory (relative to the binary or from cargo package)
pub fn get_embedded_skills_dir() -> Result<PathBuf> {
    // First, try to find skills relative to the current executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Check if we're running from the development directory (target/debug or target/release)
            let dev_skills = exe_dir.join("../../skills");
            if is_valid_skills_dir(&dev_skills) {
                return Ok(dev_skills.canonicalize()?);
            }

            // Check for skills in the same directory as the binary
            let local_skills = exe_dir.join("skills");
            if is_valid_skills_dir(&local_skills) {
                return Ok(local_skills);
            }
        }
    }

    // Try current working directory
    let cwd_skills = std::env::current_dir()?.join("skills");
    if is_valid_skills_dir(&cwd_skills) {
        return Ok(cwd_skills);
    }

    // Fallback: check if running from cargo run in the project directory
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let cargo_skills = PathBuf::from(manifest_dir).join("skills");
    if is_valid_skills_dir(&cargo_skills) {
        return Ok(cargo_skills);
    }

    anyhow::bail!("Could not find skills source directory. Run this command from the skillshub repository.")
}

/// Display a path with ~ substituted for home directory
pub fn display_path_with_tilde(path: &Path) -> String {
    if let Some(home) = get_home_dir() {
        if let Ok(stripped) = path.strip_prefix(&home) {
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_get_home_dir_uses_env_override() {
        // Save original value
        let original = std::env::var("SKILLSHUB_TEST_HOME").ok();

        // Set test override
        std::env::set_var("SKILLSHUB_TEST_HOME", "/test/home");
        let home = get_home_dir().unwrap();
        assert_eq!(home, PathBuf::from("/test/home"));

        // Restore original value
        match original {
            Some(val) => std::env::set_var("SKILLSHUB_TEST_HOME", val),
            None => std::env::remove_var("SKILLSHUB_TEST_HOME"),
        }
    }

    #[test]
    #[serial]
    fn test_get_skillshub_home() {
        let home = get_skillshub_home().unwrap();
        assert!(home.ends_with(".skillshub"));
    }

    #[test]
    #[serial]
    fn test_get_skills_install_dir() {
        let dir = get_skills_install_dir().unwrap();
        assert!(dir.ends_with("skills"));
        assert!(dir.parent().unwrap().ends_with(".skillshub"));
    }

    #[test]
    #[serial]
    fn test_get_taps_clone_dir() {
        let dir = get_taps_clone_dir().unwrap();
        assert!(dir.ends_with("taps"));
        assert!(dir.parent().unwrap().ends_with(".skillshub"));
    }

    #[test]
    #[serial]
    fn test_get_tap_clone_dir() {
        let dir = get_tap_clone_dir("owner/repo").unwrap();
        assert!(dir.ends_with("owner/repo"));
        assert!(dir.parent().unwrap().parent().unwrap().ends_with("taps"));
    }

    #[test]
    #[serial]
    fn test_display_path_with_tilde_home_path() {
        if let Some(home) = dirs::home_dir() {
            let test_path = home.join("some/nested/path");
            let display = display_path_with_tilde(&test_path);
            assert_eq!(display, "~/some/nested/path");
        }
    }

    #[test]
    #[serial]
    fn test_display_path_with_tilde_non_home_path() {
        let test_path = PathBuf::from("/usr/local/bin");
        let display = display_path_with_tilde(&test_path);
        assert_eq!(display, "/usr/local/bin");
    }
}
