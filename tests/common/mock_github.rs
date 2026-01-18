//! Mock GitHub API server for integration tests
//!
//! Uses wiremock to provide a fake GitHub API that can return
//! controlled responses for skill discovery and download operations.

use serde_json::json;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mock GitHub API server
pub struct MockGitHub {
    pub server: MockServer,
}

impl MockGitHub {
    /// Start a new mock GitHub server
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        Self { server }
    }

    /// Get the server URL for configuring test environment
    pub fn url(&self) -> String {
        self.server.uri()
    }

    /// Mock the tree API for skill discovery
    ///
    /// This simulates the GitHub Tree API response used to discover skills
    /// by scanning for SKILL.md files.
    pub async fn mock_tree_response(&self, owner: &str, repo: &str, skills: &[(&str, &str)]) {
        let tree_entries: Vec<_> = skills
            .iter()
            .map(|(path, _)| {
                json!({
                    "path": format!("{}/SKILL.md", path),
                    "type": "blob"
                })
            })
            .collect();

        let body = json!({
            "tree": tree_entries
        });

        Mock::given(method("GET"))
            .and(path_regex(format!(r"/repos/{}/{}/git/trees/.*", owner, repo)))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    /// Mock raw file content (SKILL.md)
    pub async fn mock_skill_md(&self, owner: &str, repo: &str, branch: &str, skill_path: &str, content: &str) {
        Mock::given(method("GET"))
            .and(path(format!("/{}/{}/{}/{}/SKILL.md", owner, repo, branch, skill_path)))
            .respond_with(ResponseTemplate::new(200).set_body_string(content))
            .mount(&self.server)
            .await;
    }

    /// Mock the commits API for getting latest commit SHA
    pub async fn mock_commits(&self, owner: &str, repo: &str, commit_sha: &str) {
        let body = json!([{
            "sha": commit_sha
        }]);

        Mock::given(method("GET"))
            .and(path_regex(format!(r"/repos/{}/{}/commits.*", owner, repo)))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    /// Mock tarball download
    pub async fn mock_tarball(&self, owner: &str, repo: &str, tarball_bytes: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path_regex(format!(r"/repos/{}/{}/tarball/.*", owner, repo)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(tarball_bytes))
            .mount(&self.server)
            .await;
    }

    /// Mock a 404 response for non-existent resources
    pub async fn mock_not_found(&self, path_pattern: &str) {
        Mock::given(method("GET"))
            .and(path_regex(path_pattern))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "message": "Not Found"
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock rate limit exceeded response
    pub async fn mock_rate_limit(&self) {
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_json(json!({
                        "message": "API rate limit exceeded"
                    }))
                    .insert_header("X-RateLimit-Remaining", "0"),
            )
            .mount(&self.server)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_server_starts() {
        let mock = MockGitHub::start().await;
        assert!(!mock.url().is_empty());
        assert!(mock.url().starts_with("http://"));
    }
}
