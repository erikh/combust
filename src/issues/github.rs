use anyhow::{bail, Context, Result};

use super::{Closer, Issue, Source};

/// Fetches issues from the GitHub REST API.
pub struct GitHubSource {
    pub owner: String,
    pub repo: String,
    pub token: String,
}

impl GitHubSource {
    pub fn new(owner: &str, repo: &str) -> Self {
        let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
        GitHubSource {
            owner: owner.to_string(),
            repo: repo.to_string(),
            token,
        }
    }
}

impl Source for GitHubSource {
    fn fetch_open_issues(&self, labels: &[String]) -> Result<Vec<Issue>> {
        let mut url = format!(
            "https://api.github.com/repos/{}/{}/issues?state=open&per_page=100",
            self.owner, self.repo
        );
        if !labels.is_empty() {
            url.push_str(&format!("&labels={}", labels.join(",")));
        }

        let client = reqwest::blocking::Client::new();
        let mut req = client.get(&url).header("User-Agent", "combust");
        if !self.token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.token));
        }

        let resp = req.send().context("fetching GitHub issues")?;
        if !resp.status().is_success() {
            bail!(
                "GitHub API returned status {}: {}",
                resp.status(),
                resp.text().unwrap_or_default()
            );
        }

        let items: Vec<serde_json::Value> =
            resp.json().context("parsing GitHub issues response")?;

        let mut issues = Vec::new();
        for item in &items {
            // Skip pull requests (GitHub includes them in the issues endpoint).
            if item.get("pull_request").is_some() {
                continue;
            }

            let number = item["number"].as_i64().unwrap_or(0);
            let title = item["title"].as_str().unwrap_or("").to_string();
            let body = item["body"].as_str().unwrap_or("").to_string();
            let url = item["html_url"].as_str().unwrap_or("").to_string();

            let labels: Vec<String> = item["labels"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|l| l["name"].as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            issues.push(Issue {
                number,
                title,
                body,
                labels,
                url,
            });
        }

        Ok(issues)
    }
}

impl Closer for GitHubSource {
    fn close_issue(&self, number: i64, comment: &str) -> Result<()> {
        let client = reqwest::blocking::Client::new();

        // Post comment if provided.
        if !comment.is_empty() {
            let comment_url = format!(
                "https://api.github.com/repos/{}/{}/issues/{}/comments",
                self.owner, self.repo, number
            );
            let mut req = client
                .post(&comment_url)
                .header("User-Agent", "combust")
                .json(&serde_json::json!({ "body": comment }));
            if !self.token.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", self.token));
            }
            let _ = req.send();
        }

        // Close the issue.
        let close_url = format!(
            "https://api.github.com/repos/{}/{}/issues/{}",
            self.owner, self.repo, number
        );
        let mut req = client
            .patch(&close_url)
            .header("User-Agent", "combust")
            .json(&serde_json::json!({ "state": "closed" }));
        if !self.token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.token));
        }

        let resp = req.send().context("closing GitHub issue")?;
        if !resp.status().is_success() {
            bail!("failed to close GitHub issue #{}", number);
        }

        Ok(())
    }
}

/// Parses a GitHub URL into (owner, repo).
pub fn parse_github_url(remote_url: &str) -> Result<(String, String)> {
    // SSH: git@github.com:owner/repo.git
    if let Some(rest) = remote_url.strip_prefix("git@github.com:") {
        return parse_owner_repo(rest);
    }

    // HTTPS: https://github.com/owner/repo[.git]
    for prefix in &["https://github.com/", "http://github.com/"] {
        if let Some(rest) = remote_url.strip_prefix(prefix) {
            return parse_owner_repo(rest);
        }
    }

    bail!("cannot parse GitHub URL: {}", remote_url)
}

fn parse_owner_repo(path: &str) -> Result<(String, String)> {
    let path = path.trim_end_matches(".git").trim_matches('/');
    let parts: Vec<&str> = path.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!("invalid owner/repo path: {}", path);
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_url_https() {
        let (owner, repo) = parse_github_url("https://github.com/erikh/combust").unwrap();
        assert_eq!(owner, "erikh");
        assert_eq!(repo, "combust");
    }

    #[test]
    fn test_parse_github_url_https_with_git() {
        let (owner, repo) = parse_github_url("https://github.com/erikh/combust.git").unwrap();
        assert_eq!(owner, "erikh");
        assert_eq!(repo, "combust");
    }

    #[test]
    fn test_parse_github_url_ssh() {
        let (owner, repo) = parse_github_url("git@github.com:erikh/combust.git").unwrap();
        assert_eq!(owner, "erikh");
        assert_eq!(repo, "combust");
    }

    #[test]
    fn test_parse_github_url_invalid() {
        assert!(parse_github_url("https://gitlab.com/erikh/combust").is_err());
    }
}
