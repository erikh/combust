use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// A promise within a milestone: a heading with body text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Promise {
    pub heading: String,
    pub body: String,
    pub slug: String,
    pub completed: bool,
}

impl Promise {
    /// Returns the display name (heading text).
    pub fn task_name(&self) -> &str {
        &self.heading
    }
}

/// Result of verifying a milestone's promises.
#[derive(Debug)]
pub struct VerifyResult {
    pub promises: Vec<Promise>,
    pub all_met: bool,
}

/// Result of repairing a milestone (creating missing tasks).
#[derive(Debug)]
pub struct RepairResult {
    pub created: Vec<String>,
}

/// A delivered milestone record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneHistory {
    pub date: String,
    pub content: String,
    pub delivered_at: String,
    #[serde(default)]
    pub score: String,
}

/// Manages milestones in the design directory.
pub struct Milestones {
    pub dir: PathBuf,
}

impl Milestones {
    pub fn new(design_dir: &Path) -> Self {
        Milestones {
            dir: design_dir.join("milestone"),
        }
    }

    /// Lists outstanding (undelivered) milestones by date.
    pub fn list(&self) -> Result<Vec<String>> {
        let mut dates = Vec::new();
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(dates),
            Err(e) => return Err(e).context("reading milestone directory"),
        };

        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".md") && entry.file_type()?.is_file() {
                let date = name.trim_end_matches(".md").to_string();
                dates.push(date);
            }
        }
        dates.sort();
        Ok(dates)
    }

    /// Lists delivered milestones.
    pub fn delivered(&self) -> Result<Vec<String>> {
        let delivered_dir = self.dir.join("delivered");
        let mut dates = Vec::new();
        let entries = match fs::read_dir(&delivered_dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(dates),
            Err(e) => return Err(e).context("reading delivered directory"),
        };

        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".md") && entry.file_type()?.is_file() {
                let date = name.trim_end_matches(".md").to_string();
                dates.push(date);
            }
        }
        dates.sort();
        Ok(dates)
    }

    /// Returns the content of a milestone file.
    pub fn view(&self, date: &str) -> Result<String> {
        let path = self.dir.join(format!("{}.md", date));
        if !path.exists() {
            bail!("milestone {} not found", date);
        }
        fs::read_to_string(&path).context("reading milestone file")
    }

    /// Searches for a milestone by date in both outstanding and delivered.
    pub fn find(&self, date: &str) -> Result<String> {
        let path = self.dir.join(format!("{}.md", date));
        if path.exists() {
            return fs::read_to_string(&path).context("reading milestone file");
        }
        let delivered_path = self.dir.join("delivered").join(format!("{}.md", date));
        if delivered_path.exists() {
            return fs::read_to_string(&delivered_path).context("reading delivered milestone");
        }
        bail!("milestone {} not found", date)
    }

    /// Creates a new milestone file. Returns the path.
    pub fn create(&self, date: &str, content: &str) -> Result<PathBuf> {
        fs::create_dir_all(&self.dir).context("creating milestone directory")?;
        let path = self.dir.join(format!("{}.md", date));
        if path.exists() {
            bail!("milestone {} already exists", date);
        }
        fs::write(&path, content).context("writing milestone file")?;
        Ok(path)
    }

    /// Returns the file path for a milestone (for editing).
    pub fn path(&self, date: &str) -> Result<PathBuf> {
        let path = self.dir.join(format!("{}.md", date));
        if !path.exists() {
            bail!("milestone {} not found", date);
        }
        Ok(path)
    }

    /// Parses promises from a milestone's markdown content.
    /// Promises are ## headings with body text between them.
    /// HTML comments are stripped from the content.
    pub fn parse_promises(&self, content: &str) -> Vec<Promise> {
        let stripped = strip_html_comments(content);
        let mut promises = Vec::new();
        let mut current_heading: Option<String> = None;
        let mut current_body = String::new();

        for line in stripped.lines() {
            if let Some(heading_text) = line.strip_prefix("## ") {
                // Save previous promise if any.
                if let Some(heading) = current_heading.take() {
                    let heading = heading.trim().to_string();
                    if !heading.is_empty() {
                        promises.push(Promise {
                            slug: slugify(&heading),
                            heading,
                            body: current_body.trim().to_string(),
                            completed: false,
                        });
                    }
                }
                current_heading = Some(heading_text.to_string());
                current_body = String::new();
            } else if current_heading.is_some() {
                current_body.push_str(line);
                current_body.push('\n');
            }
        }

        // Save last promise.
        if let Some(heading) = current_heading {
            let heading = heading.trim().to_string();
            if !heading.is_empty() {
                promises.push(Promise {
                    slug: slugify(&heading),
                    heading,
                    body: current_body.trim().to_string(),
                    completed: false,
                });
            }
        }

        promises
    }

    /// Verifies whether all promised tasks are completed.
    /// Looks up tasks by slug in the milestone task group, then falls back
    /// to checking all state directories by name.
    pub fn verify(
        &self,
        date: &str,
        design_dir: &super::Dir,
    ) -> Result<VerifyResult> {
        let content = self.view(date)?;
        let mut promises = self.parse_promises(&content);

        let task_group = milestone_task_group(date);

        // Get completed tasks.
        let completed = design_dir
            .tasks_by_state(super::task::TaskState::Completed)?;
        let completed_labels: std::collections::HashSet<String> =
            completed.iter().map(|t| t.label()).collect();
        let completed_names: std::collections::HashSet<String> =
            completed.iter().map(|t| t.name.clone()).collect();

        let mut all_met = true;

        for promise in &mut promises {
            // Check by group/slug label.
            let grouped_label = format!("{}/{}", task_group, promise.slug);
            let is_completed = completed_labels.contains(&grouped_label)
                || completed_names.contains(&promise.slug)
                || completed_labels.contains(&promise.slug);

            promise.completed = is_completed;
            if !is_completed {
                all_met = false;
            }
        }

        Ok(VerifyResult { promises, all_met })
    }

    /// Creates missing task files for promises that don't exist anywhere.
    /// Tasks are created in the milestone task group directory.
    pub fn repair(
        &self,
        date: &str,
        design_dir: &super::Dir,
    ) -> Result<RepairResult> {
        let content = self.view(date)?;
        let promises = self.parse_promises(&content);

        let task_group = milestone_task_group(date);

        let all_tasks = design_dir.all_tasks()?;
        let existing_labels: std::collections::HashSet<String> =
            all_tasks.iter().map(|t| t.label()).collect();
        let existing_names: std::collections::HashSet<String> =
            all_tasks.iter().map(|t| t.name.clone()).collect();

        let mut created = Vec::new();

        let group_dir = design_dir.path.join("tasks").join(&task_group);
        fs::create_dir_all(&group_dir)
            .with_context(|| format!("creating task group dir for {}", task_group))?;

        // Ensure group.md exists.
        let group_md = group_dir.join("group.md");
        if !group_md.exists() {
            fs::write(
                &group_md,
                format!("Tasks for milestone {}.\n", date),
            )
            .context("creating group.md for milestone")?;
        }

        for promise in &promises {
            let grouped_label = format!("{}/{}", task_group, promise.slug);
            if existing_labels.contains(&grouped_label)
                || existing_names.contains(&promise.slug)
            {
                continue;
            }

            let task_path = group_dir.join(format!("{}.md", promise.slug));
            let task_content = if promise.body.is_empty() {
                format!("# {}\n\nTODO: Define this task.\n", promise.heading)
            } else {
                format!("# {}\n\n{}\n", promise.heading, promise.body)
            };
            fs::write(&task_path, task_content)
                .with_context(|| format!("creating task file for {}", promise.slug))?;

            created.push(grouped_label);
        }

        Ok(RepairResult { created })
    }

    /// Marks a milestone as delivered by moving it to the delivered/ directory.
    pub fn deliver(&self, date: &str) -> Result<()> {
        self.deliver_with_score(date, "")
    }

    /// Marks a milestone as delivered with an optional A-F score.
    pub fn deliver_with_score(&self, date: &str, score: &str) -> Result<()> {
        let src = self.dir.join(format!("{}.md", date));
        if !src.exists() {
            bail!("milestone {} not found", date);
        }

        let delivered_dir = self.dir.join("delivered");
        fs::create_dir_all(&delivered_dir).context("creating delivered directory")?;

        let dest = delivered_dir.join(format!("{}.md", date));
        fs::rename(&src, &dest).context("moving milestone to delivered")?;

        // Write history record.
        let history_dir = self.dir.join("history");
        fs::create_dir_all(&history_dir).context("creating history directory")?;

        let content = fs::read_to_string(&dest).context("reading delivered milestone")?;
        let history = MilestoneHistory {
            date: date.to_string(),
            content,
            delivered_at: chrono_now(),
            score: score.to_string(),
        };

        let history_filename = if score.is_empty() {
            format!("{}.json", date)
        } else {
            format!("{}-{}.json", date, score)
        };
        let history_path = history_dir.join(history_filename);
        let data = serde_json::to_string_pretty(&history).context("marshaling history")?;
        fs::write(&history_path, data).context("writing history record")?;

        Ok(())
    }

    /// Returns the history for a milestone.
    pub fn history(&self, date: &str) -> Result<MilestoneHistory> {
        let history_dir = self.dir.join("history");

        // Try exact match first.
        let exact_path = history_dir.join(format!("{}.json", date));
        if exact_path.exists() {
            let data = fs::read_to_string(&exact_path)
                .with_context(|| format!("reading history for {}", date))?;
            return serde_json::from_str(&data).context("parsing history record");
        }

        // Search for {date}-{score}.json pattern.
        let entries = match fs::read_dir(&history_dir) {
            Ok(e) => e,
            Err(e) => return Err(e).with_context(|| format!("reading history for {}", date)),
        };

        let prefix = format!("{}-", date);
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) && name.ends_with(".json") {
                let data = fs::read_to_string(entry.path())
                    .with_context(|| format!("reading history for {}", date))?;
                return serde_json::from_str(&data).context("parsing history record");
            }
        }

        bail!("no history found for milestone {}", date)
    }
}

/// Returns the current date/time as a string.
fn chrono_now() -> String {
    // Simple ISO-ish timestamp without pulling in chrono crate.
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

/// Returns the task group name for a milestone date.
pub fn milestone_task_group(date: &str) -> String {
    format!("milestone-{}", date)
}

/// Strips HTML comments (<!-- ... -->) from content.
pub fn strip_html_comments(content: &str) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c == '<' {
            // Check for <!--
            let mut potential = String::new();
            potential.push(chars.next().unwrap());

            let mut is_comment = true;
            for expected in ['!', '-', '-'] {
                match chars.peek() {
                    Some(&ch) if ch == expected => {
                        potential.push(chars.next().unwrap());
                    }
                    _ => {
                        is_comment = false;
                        break;
                    }
                }
            }

            if is_comment {
                // Skip until -->
                let mut dash_count = 0;
                for c in chars.by_ref() {
                    if c == '-' {
                        dash_count += 1;
                    } else if c == '>' && dash_count >= 2 {
                        break;
                    } else {
                        dash_count = 0;
                    }
                }
            } else {
                result.push_str(&potential);
            }
        } else {
            result.push(chars.next().unwrap());
        }
    }

    result
}

/// Normalizes a date string for use as a milestone filename.
/// Supports: YYYY-MM-DD, YYYY/MM/DD, MM-DD-YYYY, MM/DD/YYYY.
pub fn normalize_date(date: &str) -> Result<String> {
    let trimmed = date.trim();

    // Try splitting by - or /
    let parts: Vec<&str> = trimmed.split(['-', '/']).collect();

    if parts.len() != 3 {
        bail!("invalid date format: {:?} (expected YYYY-MM-DD, YYYY/MM/DD, MM-DD-YYYY, or MM/DD/YYYY)", trimmed);
    }

    let (year, month, day) = if parts[0].len() == 4 {
        // YYYY-MM-DD or YYYY/MM/DD
        (parts[0], parts[1], parts[2])
    } else if parts[2].len() == 4 {
        // MM-DD-YYYY or MM/DD/YYYY
        (parts[2], parts[0], parts[1])
    } else {
        bail!("invalid date format: {:?}", trimmed);
    };

    let y: u32 = year.parse().map_err(|_| anyhow::anyhow!("invalid year: {}", year))?;
    let m: u32 = month.parse().map_err(|_| anyhow::anyhow!("invalid month: {}", month))?;
    let d: u32 = day.parse().map_err(|_| anyhow::anyhow!("invalid day: {}", day))?;

    if !(1900..=9999).contains(&y) {
        bail!("year out of range: {}", y);
    }
    if !(1..=12).contains(&m) {
        bail!("month out of range: {}", m);
    }
    if !(1..=31).contains(&d) {
        bail!("day out of range: {}", d);
    }

    Ok(format!("{:04}-{:02}-{:02}", y, m, d))
}

/// Converts a string to a URL-friendly slug.
pub fn slugify(s: &str) -> String {
    let mut slug = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if (c == ' ' || c == '-' || c == '_') && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.truncate(60);
    slug.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Milestones) {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("milestone/history")).unwrap();
        fs::create_dir_all(tmp.path().join("milestone/delivered")).unwrap();
        let ms = Milestones::new(tmp.path());
        (tmp, ms)
    }

    fn setup_with_design() -> (TempDir, Milestones, super::super::Dir) {
        let tmp = TempDir::new().unwrap();
        super::super::scaffold(tmp.path()).unwrap();
        let ms = Milestones::new(tmp.path());
        let dir = super::super::Dir::new(tmp.path()).unwrap();
        (tmp, ms, dir)
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Add Auth Feature"), "add-auth-feature");
        assert_eq!(slugify("Fix Bug #123"), "fix-bug-123");
        assert_eq!(slugify("  spaces  "), "spaces");
    }

    #[test]
    fn test_slugify_long_input() {
        let long_input = "a".repeat(100);
        let result = slugify(&long_input);
        assert!(result.len() <= 60);
    }

    #[test]
    fn test_normalize_date_yyyy_mm_dd() {
        assert_eq!(normalize_date("2024-01-15").unwrap(), "2024-01-15");
    }

    #[test]
    fn test_normalize_date_yyyy_slash() {
        assert_eq!(normalize_date("2024/01/15").unwrap(), "2024-01-15");
    }

    #[test]
    fn test_normalize_date_mm_dd_yyyy() {
        assert_eq!(normalize_date("01-15-2024").unwrap(), "2024-01-15");
    }

    #[test]
    fn test_normalize_date_mm_slash_dd_slash_yyyy() {
        assert_eq!(normalize_date("01/15/2024").unwrap(), "2024-01-15");
    }

    #[test]
    fn test_normalize_date_invalid() {
        assert!(normalize_date("not-a-date").is_err());
        assert!(normalize_date("2024-13-01").is_err());
        assert!(normalize_date("2024-01-32").is_err());
        assert!(normalize_date("2024-00-15").is_err());
    }

    #[test]
    fn test_list_empty() {
        let (_tmp, ms) = setup();
        assert!(ms.list().unwrap().is_empty());
    }

    #[test]
    fn test_create_and_list() {
        let (_tmp, ms) = setup();
        ms.create("2024-01-15", "## Task A\n\nDo A\n## Task B\n\nDo B\n").unwrap();
        ms.create("2024-02-01", "## Task C\n\nDo C\n").unwrap();
        let dates = ms.list().unwrap();
        assert_eq!(dates, vec!["2024-01-15", "2024-02-01"]);
    }

    #[test]
    fn test_create_duplicate() {
        let (_tmp, ms) = setup();
        ms.create("2024-01-15", "content").unwrap();
        let result = ms.create("2024-01-15", "other");
        assert!(result.is_err());
    }

    #[test]
    fn test_view() {
        let (_tmp, ms) = setup();
        ms.create("2024-01-15", "milestone content").unwrap();
        let content = ms.view("2024-01-15").unwrap();
        assert_eq!(content, "milestone content");
    }

    #[test]
    fn test_view_not_found() {
        let (_tmp, ms) = setup();
        assert!(ms.view("nonexistent").is_err());
    }

    #[test]
    fn test_milestone_content() {
        let (_tmp, ms) = setup();
        let content = "# Milestone 2024-01-15\n\n## Add Login\n\nImplement login page.\n\n## Fix Tests\n\nFix all broken tests.\n";
        ms.create("2024-01-15", content).unwrap();
        let retrieved = ms.view("2024-01-15").unwrap();
        assert_eq!(retrieved, content);
    }

    #[test]
    fn test_find_milestone() {
        let (_tmp, ms) = setup();
        ms.create("2024-01-15", "content").unwrap();
        let found = ms.find("2024-01-15").unwrap();
        assert_eq!(found, "content");

        // After delivery, should still be findable.
        ms.deliver("2024-01-15").unwrap();
        let found = ms.find("2024-01-15").unwrap();
        assert_eq!(found, "content");
    }

    #[test]
    fn test_parse_promises_multi_heading() {
        let (_tmp, ms) = setup();
        let content = "# Milestone\n\n## Add Login\n\nImplement the login feature.\n\n## Fix Tests\n\nFix all broken tests.\n";
        let promises = ms.parse_promises(content);
        assert_eq!(promises.len(), 2);
        assert_eq!(promises[0].heading, "Add Login");
        assert_eq!(promises[0].body, "Implement the login feature.");
        assert_eq!(promises[0].slug, "add-login");
        assert_eq!(promises[1].heading, "Fix Tests");
        assert_eq!(promises[1].body, "Fix all broken tests.");
        assert_eq!(promises[1].slug, "fix-tests");
    }

    #[test]
    fn test_parse_promises_single() {
        let (_tmp, ms) = setup();
        let content = "## Single Feature\n\nDo the thing.\n";
        let promises = ms.parse_promises(content);
        assert_eq!(promises.len(), 1);
        assert_eq!(promises[0].heading, "Single Feature");
    }

    #[test]
    fn test_parse_promises_empty() {
        let (_tmp, ms) = setup();
        let content = "# Just a title\n\nSome body text.\n";
        let promises = ms.parse_promises(content);
        assert!(promises.is_empty());
    }

    #[test]
    fn test_parse_promises_with_html_comments() {
        let (_tmp, ms) = setup();
        let content = "# Milestone\n\n<!-- This is a comment -->\n## Real Feature\n\nDo it.\n\n<!-- ## Commented Feature\n\nNot real. -->\n";
        let promises = ms.parse_promises(content);
        assert_eq!(promises.len(), 1);
        assert_eq!(promises[0].heading, "Real Feature");
    }

    #[test]
    fn test_parse_promises_empty_heading() {
        let (_tmp, ms) = setup();
        let content = "## \n\nBody for empty heading.\n## Real\n\nReal body.\n";
        let promises = ms.parse_promises(content);
        assert_eq!(promises.len(), 1);
        assert_eq!(promises[0].heading, "Real");
    }

    #[test]
    fn test_strip_html_comments() {
        let input = "before <!-- comment --> after";
        let result = strip_html_comments(input);
        assert_eq!(result, "before  after");
    }

    #[test]
    fn test_strip_html_comments_multiline() {
        let input = "before\n<!-- multi\nline\ncomment -->\nafter";
        let result = strip_html_comments(input);
        assert_eq!(result, "before\n\nafter");
    }

    #[test]
    fn test_strip_html_comments_no_comments() {
        let input = "no comments here";
        let result = strip_html_comments(input);
        assert_eq!(result, "no comments here");
    }

    #[test]
    fn test_deliver() {
        let (_tmp, ms) = setup();
        ms.create("2024-01-15", "## task-a\n\nbody\n").unwrap();
        ms.deliver("2024-01-15").unwrap();

        assert!(ms.list().unwrap().is_empty());
        assert_eq!(ms.delivered().unwrap(), vec!["2024-01-15"]);
    }

    #[test]
    fn test_deliver_with_score() {
        let (_tmp, ms) = setup();
        ms.create("2024-01-15", "## task-a\n\nbody\n").unwrap();
        ms.deliver_with_score("2024-01-15", "A").unwrap();

        assert!(ms.list().unwrap().is_empty());
        assert_eq!(ms.delivered().unwrap(), vec!["2024-01-15"]);

        // History should be findable.
        let h = ms.history("2024-01-15").unwrap();
        assert_eq!(h.score, "A");
    }

    #[test]
    fn test_history() {
        let (_tmp, ms) = setup();
        ms.create("2024-01-15", "## task-a\n\nbody\n").unwrap();
        ms.deliver("2024-01-15").unwrap();

        let h = ms.history("2024-01-15").unwrap();
        assert_eq!(h.date, "2024-01-15");
        assert!(h.content.contains("## task-a"));
    }

    #[test]
    fn test_history_with_score() {
        let (_tmp, ms) = setup();
        ms.create("2024-02-01", "## feature\n\nbody\n").unwrap();
        ms.deliver_with_score("2024-02-01", "B").unwrap();

        let h = ms.history("2024-02-01").unwrap();
        assert_eq!(h.date, "2024-02-01");
        assert_eq!(h.score, "B");
    }

    #[test]
    fn test_delivered_milestones_populated() {
        let (_tmp, ms) = setup();
        ms.create("2024-01-01", "## a\n\nb\n").unwrap();
        ms.create("2024-02-01", "## c\n\nd\n").unwrap();
        ms.deliver("2024-01-01").unwrap();
        ms.deliver("2024-02-01").unwrap();

        let delivered = ms.delivered().unwrap();
        assert_eq!(delivered.len(), 2);
    }

    #[test]
    fn test_delivered_milestones_empty() {
        let (_tmp, ms) = setup();
        let delivered = ms.delivered().unwrap();
        assert!(delivered.is_empty());
    }

    #[test]
    fn test_milestone_task_group() {
        assert_eq!(milestone_task_group("2024-01-15"), "milestone-2024-01-15");
    }

    #[test]
    fn test_verify_all_kept() {
        let (tmp, ms, dir) = setup_with_design();

        let content = "## Add Feature\n\nDo it.\n";
        ms.create("2024-01-15", content).unwrap();

        // Create the task in completed state matching the slug.
        let group = milestone_task_group("2024-01-15");
        fs::create_dir_all(tmp.path().join("state/completed").join(&group)).unwrap();
        fs::write(
            tmp.path().join("state/completed").join(&group).join("add-feature.md"),
            "done",
        )
        .unwrap();

        let result = ms.verify("2024-01-15", &dir).unwrap();
        assert!(result.all_met);
        assert!(result.promises[0].completed);
    }

    #[test]
    fn test_verify_incomplete() {
        let (_tmp, ms, dir) = setup_with_design();

        let content = "## Add Feature\n\nDo it.\n## Fix Bug\n\nFix it.\n";
        ms.create("2024-01-15", content).unwrap();

        let result = ms.verify("2024-01-15", &dir).unwrap();
        assert!(!result.all_met);
        assert_eq!(result.promises.len(), 2);
        assert!(!result.promises[0].completed);
        assert!(!result.promises[1].completed);
    }

    #[test]
    fn test_verify_missing() {
        let (_tmp, ms, dir) = setup_with_design();

        let content = "## Nonexistent Task\n\nWon't be found.\n";
        ms.create("2024-01-15", content).unwrap();

        let result = ms.verify("2024-01-15", &dir).unwrap();
        assert!(!result.all_met);
        assert!(!result.promises[0].completed);
    }

    #[test]
    fn test_repair_creates_missing() {
        let (_tmp, ms, dir) = setup_with_design();

        let content = "## Add Login\n\nImplement login.\n## Fix Tests\n\nFix them.\n";
        ms.create("2024-01-15", content).unwrap();

        let result = ms.repair("2024-01-15", &dir).unwrap();
        assert_eq!(result.created.len(), 2);

        // Verify task files were created in the milestone group.
        let group = milestone_task_group("2024-01-15");
        assert!(dir.path.join("tasks").join(&group).join("add-login.md").exists());
        assert!(dir.path.join("tasks").join(&group).join("fix-tests.md").exists());
        assert!(dir.path.join("tasks").join(&group).join("group.md").exists());

        // Verify task content includes the heading and body.
        let task_content = fs::read_to_string(
            dir.path.join("tasks").join(&group).join("add-login.md"),
        )
        .unwrap();
        assert!(task_content.contains("# Add Login"));
        assert!(task_content.contains("Implement login."));
    }

    #[test]
    fn test_repair_skips_existing() {
        let (_tmp, ms, dir) = setup_with_design();

        let content = "## Add Login\n\nImplement login.\n";
        ms.create("2024-01-15", content).unwrap();

        // Create the task first.
        let group = milestone_task_group("2024-01-15");
        let group_dir = dir.path.join("tasks").join(&group);
        fs::create_dir_all(&group_dir).unwrap();
        fs::write(group_dir.join("add-login.md"), "existing").unwrap();

        let result = ms.repair("2024-01-15", &dir).unwrap();
        assert!(result.created.is_empty());
    }
}
