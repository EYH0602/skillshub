use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// The main database stored at ~/.skillshub/db.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Database {
    /// Configured taps (name -> tap info)
    #[serde(default)]
    pub taps: HashMap<String, TapInfo>,

    /// Installed skills (full name "tap/skill" -> installation info)
    #[serde(default)]
    pub installed: HashMap<String, InstalledSkill>,

    /// External skills (skill name -> external skill info)
    /// These are skills found in agent directories that weren't installed via skillshub
    #[serde(default)]
    pub external: HashMap<String, ExternalSkill>,
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

    /// Whether this tap is preconfigured by default
    #[serde(default)]
    pub is_default: bool,

    /// Whether this tap is bundled locally with the binary
    #[serde(default)]
    pub is_bundled: bool,

    /// Cached skill registry to avoid repeated GitHub API calls
    /// This is populated when the tap is added or updated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_registry: Option<TapRegistry>,
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

/// Information about an externally-managed skill (not installed via skillshub)
/// These are skills found in agent directories that are managed elsewhere
/// (e.g., Claude marketplace, manual installation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSkill {
    /// The skill name (directory name)
    pub name: String,

    /// The agent that owns/manages this skill (e.g., ".claude")
    pub source_agent: String,

    /// Full path to the skill directory
    pub source_path: PathBuf,

    /// When this skill was discovered
    pub discovered_at: DateTime<Utc>,
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
            .and_then(|p| p.split('/').next_back())
            .map(|s| s.to_string())
    }

    /// Get the full name for use as tap name (owner/repo format)
    pub fn tap_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    /// Get the base URL for display (without /tree/branch/path)
    pub fn base_url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.repo)
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
    /// Parse a skill ID from string
    /// Supports formats:
    /// - "owner/repo/skill" (new format with owner/repo tap names)
    /// - "tap/skill" (legacy format)
    /// - "owner/repo/skill@commit" (with commit suffix)
    pub fn parse(s: &str) -> Option<Self> {
        // Remove optional @commit suffix for parsing
        let base = s.split('@').next().unwrap_or(s);
        let parts: Vec<&str> = base.split('/').collect();

        match parts.len() {
            // owner/repo/skill format (new)
            3 if !parts[0].is_empty() && !parts[1].is_empty() && !parts[2].is_empty() => Some(Self {
                tap: format!("{}/{}", parts[0], parts[1]),
                skill: parts[2].to_string(),
            }),
            // tap/skill format (legacy)
            2 if !parts[0].is_empty() && !parts[1].is_empty() => Some(Self {
                tap: parts[0].to_string(),
                skill: parts[1].to_string(),
            }),
            _ => None,
        }
    }

    /// Parse commit from skill ID (e.g., "owner/repo/skill@abc123" -> Some("abc123"))
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
    fn test_skill_id_parse_legacy_format() {
        // Legacy tap/skill format
        let id = SkillId::parse("skillshub/code-reviewer").unwrap();
        assert_eq!(id.tap, "skillshub");
        assert_eq!(id.skill, "code-reviewer");
    }

    #[test]
    fn test_skill_id_parse_new_format() {
        // New owner/repo/skill format
        let id = SkillId::parse("EYH0602/skillshub/code-reviewer").unwrap();
        assert_eq!(id.tap, "EYH0602/skillshub");
        assert_eq!(id.skill, "code-reviewer");
    }

    #[test]
    fn test_skill_id_parse_with_commit() {
        // Legacy format with commit
        let id = SkillId::parse("tap/skill@abc123").unwrap();
        assert_eq!(id.tap, "tap");
        assert_eq!(id.skill, "skill");

        // New format with commit
        let id2 = SkillId::parse("owner/repo/skill@abc123").unwrap();
        assert_eq!(id2.tap, "owner/repo");
        assert_eq!(id2.skill, "skill");

        let commit = SkillId::parse_commit("owner/repo/skill@abc123");
        assert_eq!(commit, Some("abc123".to_string()));
    }

    #[test]
    fn test_skill_id_parse_invalid() {
        assert!(SkillId::parse("no-slash").is_none());
        assert!(SkillId::parse("/skill").is_none());
        assert!(SkillId::parse("tap/").is_none());
        assert!(SkillId::parse("").is_none());
        assert!(SkillId::parse("a/b/c/d").is_none()); // too many parts
    }

    #[test]
    fn test_skill_id_full_name() {
        let id = SkillId {
            tap: "owner/repo".to_string(),
            skill: "my-skill".to_string(),
        };
        assert_eq!(id.full_name(), "owner/repo/my-skill");
    }

    #[test]
    fn test_github_url_methods() {
        let url = GitHubUrl {
            owner: "user".to_string(),
            repo: "repo".to_string(),
            branch: "main".to_string(),
            path: Some("skills".to_string()),
        };

        assert_eq!(url.tap_name(), "user/repo");
        assert_eq!(url.base_url(), "https://github.com/user/repo");
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
        assert!(db.external.is_empty());
    }

    #[test]
    fn test_tap_info_serialize() {
        let tap = TapInfo {
            url: "https://github.com/user/repo".to_string(),
            skills_path: "skills".to_string(),
            updated_at: None,
            is_default: false,
            is_bundled: false,
            cached_registry: None,
        };

        let json = serde_json::to_string(&tap).unwrap();
        assert!(json.contains("user/repo"));
        // cached_registry should be skipped when None
        assert!(!json.contains("cached_registry"));
    }

    #[test]
    fn test_tap_info_with_cached_registry() {
        let mut skills = HashMap::new();
        skills.insert(
            "my-skill".to_string(),
            SkillEntry {
                path: "skills/my-skill".to_string(),
                description: Some("A test skill".to_string()),
                homepage: None,
            },
        );

        let registry = TapRegistry {
            name: "test-tap".to_string(),
            description: Some("Test tap".to_string()),
            skills,
        };

        let tap = TapInfo {
            url: "https://github.com/user/repo".to_string(),
            skills_path: "skills".to_string(),
            updated_at: None,
            is_default: false,
            is_bundled: false,
            cached_registry: Some(registry),
        };

        let json = serde_json::to_string(&tap).unwrap();
        assert!(json.contains("cached_registry"));
        assert!(json.contains("my-skill"));
        assert!(json.contains("A test skill"));
    }

    #[test]
    fn test_tap_info_deserialize_without_cache() {
        // Simulate loading old database format without cached_registry field
        let json = r#"{
            "url": "https://github.com/user/repo",
            "skills_path": "skills",
            "updated_at": null,
            "is_default": false,
            "is_bundled": false
        }"#;

        let tap: TapInfo = serde_json::from_str(json).unwrap();
        assert!(tap.cached_registry.is_none());
    }

    #[test]
    fn test_tap_info_roundtrip_with_cache() {
        let mut skills = HashMap::new();
        skills.insert(
            "skill1".to_string(),
            SkillEntry {
                path: "skills/skill1".to_string(),
                description: Some("First skill".to_string()),
                homepage: Some("https://example.com".to_string()),
            },
        );
        skills.insert(
            "skill2".to_string(),
            SkillEntry {
                path: "other/skill2".to_string(),
                description: None,
                homepage: None,
            },
        );

        let registry = TapRegistry {
            name: "my-tap".to_string(),
            description: None,
            skills,
        };

        let tap = TapInfo {
            url: "https://github.com/owner/repo".to_string(),
            skills_path: "skills".to_string(),
            updated_at: Some(chrono::Utc::now()),
            is_default: false,
            is_bundled: false,
            cached_registry: Some(registry),
        };

        // Serialize and deserialize
        let json = serde_json::to_string(&tap).unwrap();
        let restored: TapInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.url, tap.url);
        assert!(restored.cached_registry.is_some());
        let cached = restored.cached_registry.unwrap();
        assert_eq!(cached.name, "my-tap");
        assert_eq!(cached.skills.len(), 2);
        assert!(cached.skills.contains_key("skill1"));
        assert!(cached.skills.contains_key("skill2"));
    }
}
