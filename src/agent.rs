use std::path::PathBuf;
use tabled::Tabled;

use crate::paths::get_home_dir;

/// Agent configuration: (agent_dir, skills_subdir)
pub const KNOWN_AGENTS: &[(&str, &str)] = &[
    (".claude", "skills"),
    (".codex", "skills"),
    (".opencode", "skill"),
    (".aider", "skills"),
    (".cursor", "skills"),
    (".continue", "skills"),
];

/// Discovered agent info
pub struct AgentInfo {
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
        for (agent_dir, skills_subdir) in KNOWN_AGENTS {
            let agent_path = home.join(agent_dir);
            if agent_path.exists() && agent_path.is_dir() {
                agents.push(AgentInfo {
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
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_agents_have_skills_subdir() {
        for (agent, subdir) in KNOWN_AGENTS {
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
    }

    #[test]
    fn test_known_agent_names_format() {
        let names = known_agent_names();
        // Should be comma-separated
        assert!(names.contains(", "));
    }

    #[test]
    fn test_discover_agents_returns_vec() {
        // This test just verifies the function doesn't panic
        // and returns a valid Vec (may be empty if no agents installed)
        let agents = discover_agents();
        // Each agent should have a valid path and subdir
        for agent in agents {
            assert!(!agent.skills_subdir.is_empty());
            assert!(agent.path.exists());
        }
    }
}
