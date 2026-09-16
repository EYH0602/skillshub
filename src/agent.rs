use std::path::PathBuf;
use tabled::Tabled;

use crate::paths::get_home_dir;

/// Agent configuration: (name, agent_dir, skills_subdir)
pub const KNOWN_AGENTS: &[(&str, &str, &str)] = &[
    (".claude", ".claude", "skills"),
    (".codex", ".codex", "skills"),
    (".opencode", ".opencode", "skills"),
    (".aider", ".aider", "skills"),
    (".cursor", ".cursor", "skills"),
    (".continue", ".continue", "skills"),
    (".trae", ".trae", "skills"),
    (".kimi", ".kimi", "skills"),
    (".openclaw", ".openclaw", "skills"),
    (".zeroclaw", ".zeroclaw", "skills"),
    (".kiro", ".kiro", "steering"),
    (".gemini", ".gemini", "skills"),
    (".copilot", ".copilot", "skills"),
    (".junie", ".junie", "skills"),
    (".augment", ".augment", "skills"),
    (".warp", ".warp", "skills"),
    (".cline", ".cline", "skills"),
    (".antigravity", ".gemini/config", "skills"),
];

/// Discovered agent info
pub struct AgentInfo {
    pub name: &'static str,
    pub path: PathBuf,
    pub skills_subdir: &'static str,
}

/// Table row for displaying agents
#[derive(Tabled)]
pub struct AgentRow {
    #[tabled(rename = "Agent")]
    pub name: String,
    #[tabled(rename = "Status")]
    pub status: &'static str,
    #[tabled(rename = "Skills")]
    pub skills: String,
    #[tabled(rename = "Path")]
    pub path: String,
}

/// Discover coding agents on the system
pub fn discover_agents() -> Vec<AgentInfo> {
    let mut agents = Vec::new();

    if let Some(home) = get_home_dir() {
        for (name, agent_dir, skills_subdir) in KNOWN_AGENTS {
            let agent_path = home.join(agent_dir);
            let exists = if *name == ".antigravity" {
                (agent_path.exists() && agent_path.is_dir())
                    || (home.join(".gemini/antigravity-cli").exists() && home.join(".gemini/antigravity-cli").is_dir())
            } else {
                agent_path.exists() && agent_path.is_dir()
            };
            if exists {
                agents.push(AgentInfo {
                    name,
                    path: agent_path,
                    skills_subdir,
                });
            }
        }
    }

    agents
}

/// Get a comma-separated list of known agent names
pub fn known_agent_names() -> String {
    KNOWN_AGENTS
        .iter()
        .map(|(name, _, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_agents_have_skills_subdir() {
        for (name, agent, subdir) in KNOWN_AGENTS {
            assert!(!name.is_empty());
            assert!(!agent.is_empty());
            assert!(!subdir.is_empty());
        }
    }

    #[test]
    fn test_known_agent_names() {
        let names = known_agent_names();
        assert!(names.contains(".claude"));
        assert!(names.contains(".codex"));
        assert!(names.contains(".opencode"));
        assert!(names.contains(".trae"));
        assert!(names.contains(".kimi"));
        assert!(names.contains(".openclaw"));
        assert!(names.contains(".zeroclaw"));
        assert!(names.contains(".kiro"));
        assert!(names.contains(".gemini"));
        assert!(names.contains(".copilot"));
        assert!(names.contains(".junie"));
        assert!(names.contains(".augment"));
        assert!(names.contains(".warp"));
        assert!(names.contains(".cline"));
        assert!(names.contains(".antigravity"));
    }

    #[test]
    fn test_known_agent_names_format() {
        let names = known_agent_names();
        // Should be comma-separated
        assert!(names.contains(", "));
    }

    #[test]
    fn test_discover_agents_returns_vec() {
        let agents = discover_agents();
        for agent in agents {
            assert!(!agent.skills_subdir.is_empty());
            assert!(agent.path.exists());
        }
    }
}
