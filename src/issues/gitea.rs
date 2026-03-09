use anyhow::{bail, Context, Result};

use super::{Closer, Issue, Source};

/// Fetches issues from a Gitea instance.
pub struct GiteaSource {
    pub base_url: String,
    pub owner: String,
    pub repo: String,
    pub token: String,
}

impl GiteaSource {
    pub fn new(base_url: &str, owner: &str, repo: &str) -> Self {
        let token = std::env::var("GITEA_TOKEN").unwrap_or_default();
        GiteaSource {
            base_url: base_url.trim_end_matches('/').to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            token,
        }
    }
}

impl Source for GiteaSource {
    fn fetch_open_issues(&self, labels: &[String]) -> Result<Vec<Issue>> {
        let mut url = format!(
            "{}/api/v1/repos/{}/{}/issues?state=open&type=issues&limit=50",
            self.base_url, self.owner, self.repo
        );
        if !labels.is_empty() {
            for label in labels {
                url.push_str(&format!(
                    "&labels={}",
                    urlencoding::encode(label)
                ));
            }
        }

        let client = reqwest::blocking::Client::new();
        let mut req = client.get(&url).header("User-Agent", "combust");
        if !self.token.is_empty() {
            req = req.header("Authorization", format!("token {}", self.token));
        }

        let resp = req.send().context("fetching Gitea issues")?;
        if !resp.status().is_success() {
            bail!(
                "Gitea API returned status {}: {}",
                resp.status(),
                resp.text().unwrap_or_default()
            );
        }

        let items: Vec<serde_json::Value> =
            resp.json().context("parsing Gitea issues response")?;

        let mut issues = Vec::new();
        for item in &items {
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

impl Closer for GiteaSource {
    fn close_issue(&self, number: i64, comment: &str) -> Result<()> {
        let client = reqwest::blocking::Client::new();

        // Post comment if provided.
        if !comment.is_empty() {
            let comment_url = format!(
                "{}/api/v1/repos/{}/{}/issues/{}/comments",
                self.base_url, self.owner, self.repo, number
            );
            let mut req = client
                .post(&comment_url)
                .header("User-Agent", "combust")
                .json(&serde_json::json!({ "body": comment }));
            if !self.token.is_empty() {
                req = req.header("Authorization", format!("token {}", self.token));
            }
            let _ = req.send();
        }

        // Close the issue.
        let close_url = format!(
            "{}/api/v1/repos/{}/{}/issues/{}",
            self.base_url, self.owner, self.repo, number
        );
        let mut req = client
            .patch(&close_url)
            .header("User-Agent", "combust")
            .json(&serde_json::json!({ "state": "closed" }));
        if !self.token.is_empty() {
            req = req.header("Authorization", format!("token {}", self.token));
        }

        let resp = req.send().context("closing Gitea issue")?;
        let status = resp.status();
        if !status.is_success() && status.as_u16() != 201 {
            bail!("failed to close Gitea issue #{}", number);
        }

        Ok(())
    }
}

/// Parses a Gitea URL into (base_url, owner, repo).
pub fn parse_gitea_url(remote_url: &str) -> Result<(String, String, String)> {
    // SSH: git@host:owner/repo.git
    if remote_url.starts_with("git@") {
        let rest = remote_url.strip_prefix("git@").unwrap();
        let (host, path) = rest
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid SSH URL: {}", remote_url))?;
        let (owner, repo) = parse_owner_repo_path(path)?;
        return Ok((format!("https://{}", host), owner, repo));
    }

    // HTTPS: https://host/owner/repo[.git]
    if let Some(rest) = remote_url.strip_prefix("https://") {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() >= 3 {
            let host = parts[0];
            let owner = parts[1];
            let repo = parts[2].trim_end_matches(".git");
            return Ok((
                format!("https://{}", host),
                owner.to_string(),
                repo.to_string(),
            ));
        }
    }

    if let Some(rest) = remote_url.strip_prefix("http://") {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() >= 3 {
            let host = parts[0];
            let owner = parts[1];
            let repo = parts[2].trim_end_matches(".git");
            return Ok((
                format!("http://{}", host),
                owner.to_string(),
                repo.to_string(),
            ));
        }
    }

    bail!("cannot parse Gitea URL: {}", remote_url)
}

fn parse_owner_repo_path(path: &str) -> Result<(String, String)> {
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
    fn test_parse_gitea_url_https() {
        let (base, owner, repo) =
            parse_gitea_url("https://gitea.example.com/erikh/combust").unwrap();
        assert_eq!(base, "https://gitea.example.com");
        assert_eq!(owner, "erikh");
        assert_eq!(repo, "combust");
    }

    #[test]
    fn test_parse_gitea_url_ssh() {
        let (base, owner, repo) =
            parse_gitea_url("git@gitea.example.com:erikh/combust.git").unwrap();
        assert_eq!(base, "https://gitea.example.com");
        assert_eq!(owner, "erikh");
        assert_eq!(repo, "combust");
    }

    #[test]
    fn test_parse_gitea_url_invalid() {
        assert!(parse_gitea_url("ftp://invalid").is_err());
    }
}
