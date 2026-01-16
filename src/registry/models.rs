use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The main database stored at ~/.skillshub/db.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Database {
    /// Configured taps (name -> tap info)
    #[serde(default)]
    pub taps: HashMap<String, TapInfo>,

    /// Installed skills (full name "tap/skill" -> installation info)
    #[serde(default)]
    pub installed: HashMap<String, InstalledSkill>,
}

/// Information about a configured tap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapInfo {
    /// GitHub URL of the tap repository
    pub url: String,

    /// Path to skills directory within the repo (e.g., "skills")
    pub skills_path: String,

    /// When the tap registry was last updated
    pub updated_at: Option<DateTime<Utc>>,

    /// Whether this is the default tap (bundled with skillshub)
    #[serde(default)]
    pub is_default: bool,
}

/// Information about an installed skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkill {
    /// The tap this skill came from
    pub tap: String,

    /// The skill name (without tap prefix)
    pub skill: String,

    /// Git commit SHA when installed (None for local/bundled skills)
    pub commit: Option<String>,

    /// When the skill was installed
    pub installed_at: DateTime<Utc>,

    /// Whether this skill is from local/bundled source (no download needed)
    #[serde(default)]
    pub local: bool,

    /// Source URL for the skill (for remote skills added directly)
    #[serde(default)]
    pub source_url: Option<String>,

    /// Path within the repository where this skill lives
    #[serde(default)]
    pub source_path: Option<String>,
}

/// Registry format for remote taps (registry.json in tap repo)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapRegistry {
    /// Name of the tap
    pub name: String,

    /// Optional description of the tap
    pub description: Option<String>,

    /// Skills provided by this tap (skill name -> entry)
    #[serde(default)]
    pub skills: HashMap<String, SkillEntry>,
}

/// Entry for a skill in a tap registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    /// Path to the skill within the tap repository (e.g., "skills/my-skill")
    pub path: String,

    /// Description of what this skill does
    pub description: Option<String>,

    /// Optional homepage URL
    pub homepage: Option<String>,
}

/// Parsed GitHub URL components
#[derive(Debug, Clone)]
pub struct GitHubUrl {
    /// Repository owner
    pub owner: String,

    /// Repository name
    pub repo: String,

    /// Branch or commit (default: "main")
    pub branch: String,

    /// Path within the repository (optional)
    pub path: Option<String>,
}

impl GitHubUrl {
    /// Check if the branch looks like a commit SHA (40 hex chars or 7+ hex prefix)
    pub fn is_commit_sha(&self) -> bool {
        let b = &self.branch;
        b.len() >= 7 && b.chars().all(|c| c.is_ascii_hexdigit())
    }

    /// Get the skill name from the path (last component)
    pub fn skill_name(&self) -> Option<String> {
        self.path
            .as_ref()
            .and_then(|p| p.split('/').last())
            .map(|s| s.to_string())
    }

    /// Get the full name for use as tap name (repo name)
    pub fn tap_name(&self) -> &str {
        &self.repo
    }

    /// Get the API URL for the repository
    pub fn api_url(&self) -> String {
        format!("https://api.github.com/repos/{}/{}", self.owner, self.repo)
    }

    /// Get the tarball URL for downloading
    pub fn tarball_url(&self, git_ref: &str) -> String {
        format!(
            "https://api.github.com/repos/{}/{}/tarball/{}",
            self.owner, self.repo, git_ref
        )
    }

    /// Get the raw content URL for a file
    pub fn raw_url(&self, path: &str) -> String {
        format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            self.owner, self.repo, self.branch, path
        )
    }
}

/// Full skill identifier (tap_name/skill_name)
#[derive(Debug, Clone)]
pub struct SkillId {
    pub tap: String,
    pub skill: String,
}

impl SkillId {
    /// Parse a skill ID from string (e.g., "skillshub/skill-creator")
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            // Handle optional @commit suffix
            let skill = parts[1].split('@').next().unwrap_or(parts[1]);
            Some(Self {
                tap: parts[0].to_string(),
                skill: skill.to_string(),
            })
        } else {
            None
        }
    }

    /// Parse commit from skill ID (e.g., "tap/skill@abc123" -> Some("abc123"))
    pub fn parse_commit(s: &str) -> Option<String> {
        s.split('@').nth(1).map(|s| s.to_string())
    }

    /// Get the full name (tap/skill)
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.tap, self.skill)
    }
}

impl std::fmt::Display for SkillId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.tap, self.skill)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_id_parse_valid() {
        let id = SkillId::parse("skillshub/skill-creator").unwrap();
        assert_eq!(id.tap, "skillshub");
        assert_eq!(id.skill, "skill-creator");
    }

    #[test]
    fn test_skill_id_parse_with_commit() {
        let id = SkillId::parse("tap/skill@abc123").unwrap();
        assert_eq!(id.tap, "tap");
        assert_eq!(id.skill, "skill");

        let commit = SkillId::parse_commit("tap/skill@abc123");
        assert_eq!(commit, Some("abc123".to_string()));
    }

    #[test]
    fn test_skill_id_parse_invalid() {
        assert!(SkillId::parse("no-slash").is_none());
        assert!(SkillId::parse("/skill").is_none());
        assert!(SkillId::parse("tap/").is_none());
        assert!(SkillId::parse("").is_none());
    }

    #[test]
    fn test_skill_id_full_name() {
        let id = SkillId {
            tap: "my-tap".to_string(),
            skill: "my-skill".to_string(),
        };
        assert_eq!(id.full_name(), "my-tap/my-skill");
    }

    #[test]
    fn test_github_url_methods() {
        let url = GitHubUrl {
            owner: "user".to_string(),
            repo: "repo".to_string(),
            branch: "main".to_string(),
            path: Some("skills".to_string()),
        };

        assert_eq!(url.tap_name(), "repo");
        assert_eq!(url.api_url(), "https://api.github.com/repos/user/repo");
        assert_eq!(
            url.tarball_url("main"),
            "https://api.github.com/repos/user/repo/tarball/main"
        );
        assert_eq!(
            url.raw_url("registry.json"),
            "https://raw.githubusercontent.com/user/repo/main/registry.json"
        );
    }

    #[test]
    fn test_database_default() {
        let db = Database::default();
        assert!(db.taps.is_empty());
        assert!(db.installed.is_empty());
    }

    #[test]
    fn test_tap_info_serialize() {
        let tap = TapInfo {
            url: "https://github.com/user/repo".to_string(),
            skills_path: "skills".to_string(),
            updated_at: None,
            is_default: false,
        };

        let json = serde_json::to_string(&tap).unwrap();
        assert!(json.contains("user/repo"));
    }
}
