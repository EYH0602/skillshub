use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Get the skillshub home directory (~/.skillshub)
pub fn get_skillshub_home() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".skillshub"))
}

/// Get the skills installation directory (~/.skillshub/skills)
pub fn get_skills_install_dir() -> Result<PathBuf> {
    Ok(get_skillshub_home()?.join("skills"))
}

/// Get the embedded skills directory (relative to the binary or from cargo package)
pub fn get_embedded_skills_dir() -> Result<PathBuf> {
    // First, try to find skills relative to the current executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Check if we're running from the development directory
            let dev_skills = exe_dir.join("../../skills");
            if dev_skills.exists() {
                return Ok(dev_skills.canonicalize()?);
            }

            // Check for skills in the same directory as the binary
            let local_skills = exe_dir.join("skills");
            if local_skills.exists() {
                return Ok(local_skills);
            }
        }
    }

    // Try current working directory
    let cwd_skills = std::env::current_dir()?.join("skills");
    if cwd_skills.exists() {
        return Ok(cwd_skills);
    }

    // Fallback: check if running from cargo run in the project directory
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let cargo_skills = PathBuf::from(manifest_dir).join("skills");
    if cargo_skills.exists() {
        return Ok(cargo_skills);
    }

    anyhow::bail!("Could not find skills directory. Make sure you're running from the skillshub directory or have installed skills.")
}

/// Display a path with ~ substituted for home directory
pub fn display_path_with_tilde(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = path.strip_prefix(&home) {
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
}
