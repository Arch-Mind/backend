//! Git Commit History Analyzer
//!
//! Extracts commit history and contribution metrics for files in a repository.

use anyhow::{Context, Result};
use git2::{Repository, Oid};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use chrono::{DateTime, Utc, TimeZone};
use tracing::{info, warn, debug};
use serde::Serialize;

const DEFAULT_MAX_COMMITS: usize = 1000;

/// File contribution metrics extracted from git history
#[derive(Debug, Clone)]
pub struct FileContribution {
    pub file_path: String,
    pub commit_count: usize,
    pub last_modified: DateTime<Utc>,
    pub primary_author: String,
    pub contributors: Vec<ContributorInfo>,
    pub lines_added_total: usize,
    pub lines_deleted_total: usize,
    pub lines_changed_total: usize,
}

/// Commit history record extracted from git
#[derive(Debug, Clone, Serialize)]
pub struct CommitRecord {
    pub sha: String,
    pub author_name: String,
    pub author_email: String,
    pub message: String,
    pub authored_at: DateTime<Utc>,
    pub changed_files: Vec<String>,
    pub files_changed_count: usize,
}

/// Individual contributor information
#[derive(Debug, Clone, Serialize)]
pub struct ContributorInfo {
    pub email: String,
    pub name: String,
    pub commit_count: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
}

/// Repository-wide contribution summary
#[derive(Debug, Clone)]
pub struct RepoContributions {
    pub files: HashMap<String, FileContribution>,
    pub total_commits: usize,
    pub total_contributors: usize,
    pub commits: Vec<CommitRecord>,
}

/// Analyzes git history for a repository
pub struct GitAnalyzer {
    repo: Repository,
}

impl GitAnalyzer {
    /// Create a new GitAnalyzer for the given repository path
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo = Repository::open(repo_path)
            .context(format!("Failed to open git repository at {:?}", repo_path))?;
        
        info!("📂 Opened git repository at {:?}", repo_path);
        Ok(Self { repo })
    }

    /// Extract contribution metrics for all files in the repository
    pub fn analyze_contributions(&self) -> Result<RepoContributions> {
        self.analyze_contributions_with_limit(DEFAULT_MAX_COMMITS)
    }

    /// Extract contribution metrics for all files, but store only the latest N commit records.
    pub fn analyze_contributions_with_limit(&self, max_commits: usize) -> Result<RepoContributions> {
        info!("🔍 Analyzing git commit history...");

        let mut file_stats: HashMap<String, FileStats> = HashMap::new();
        let mut all_contributors: HashSet<String> = HashSet::new();
        let mut total_commits = 0;
        let mut commits: Vec<CommitRecord> = Vec::new();

        // Walk through all commits
        let mut revwalk = self.repo.revwalk()
            .context("Failed to create revwalk")?;

        revwalk.push_head()
            .context("Failed to push HEAD")?;

        for oid in revwalk {
            let oid = oid.context("Failed to get commit OID")?;

            match self.process_commit(oid, &mut file_stats, &mut all_contributors) {
                Ok(record) => {
                    if max_commits > 0 && commits.len() < max_commits {
                        commits.push(record);
                    }
                }
                Err(e) => {
                    warn!("⚠️  Error processing commit {}: {}", oid, e);
                    continue;
                }
            }

            total_commits += 1;

            // Progress indicator for large repos
            if total_commits % 100 == 0 {
                debug!("Processed {} commits", total_commits);
            }
        }

        info!("✅ Analyzed {} commits from {} contributors",
              total_commits, all_contributors.len());

        // Convert internal stats to FileContribution
        let files = file_stats
            .into_iter()
            .map(|(path, stats)| (path.clone(), stats.to_contribution(path)))
            .collect();

        Ok(RepoContributions {
            files,
            total_commits,
            total_contributors: all_contributors.len(),
            commits,
        })
    }

    /// Process a single commit and update file statistics
    fn process_commit(
        &self,
        oid: Oid,
        file_stats: &mut HashMap<String, FileStats>,
        all_contributors: &mut HashSet<String>,
    ) -> Result<CommitRecord> {
        let commit = self.repo.find_commit(oid)
            .context("Failed to find commit")?;
        
        let author = commit.author();
        let author_email = author.email().unwrap_or("unknown").to_string();
        let author_name = author.name().unwrap_or("unknown").to_string();
        let commit_time = Utc.timestamp_opt(commit.time().seconds(), 0)
            .single()
            .unwrap_or_else(Utc::now);
        let message = commit.message().unwrap_or("").trim().to_string();

        all_contributors.insert(author_email.clone());

        // Get parent commit for diff
        let parent = if commit.parent_count() > 0 {
            Some(commit.parent(0).context("Failed to get parent commit")?)
        } else {
            None // First commit has no parent
        };

        // Calculate diff between this commit and its parent
        let diff = if let Some(parent) = parent {
            let parent_tree = parent.tree().context("Failed to get parent tree")?;
            let commit_tree = commit.tree().context("Failed to get commit tree")?;
            
            self.repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)
                .context("Failed to create diff")?
        } else {
            // First commit: diff against empty tree
            let commit_tree = commit.tree().context("Failed to get commit tree")?;
            self.repo.diff_tree_to_tree(None, Some(&commit_tree), None)
                .context("Failed to create diff for initial commit")?
        };

        // Process each file in the diff
        // First pass: collect file-level changes
        let mut files_changed: Vec<String> = Vec::new();
        diff.foreach(
            &mut |delta, _progress| {
                if let Some(path) = delta.new_file().path() {
                    let path_str = path.to_string_lossy().to_string();
                    
                    // Skip non-code files
                    if is_code_file(&path_str) {
                        files_changed.push(path_str);
                    }
                }
                true
            },
            None,
            None,
            None,
        ).context("Failed to process diff files")?;

        // Update file stats for changed files
        for path_str in files_changed {
            let stats = file_stats.entry(path_str).or_insert_with(FileStats::new);
            stats.update_from_commit(
                &author_email,
                &author_name,
                commit_time,
            );
        }

        // Second pass: collect line-level changes
        diff.foreach(
            &mut |_delta, _progress| true,
            None,
            None,
            Some(&mut |delta, _hunk, line| {
                // Count lines added/deleted
                if let Some(path) = delta.new_file().path() {
                    let path_str = path.to_string_lossy().to_string();
                    if !is_code_file(&path_str) {
                        return true;
                    }

                    if let Some(stats) = file_stats.get_mut(&path_str) {
                        match line.origin() {
                            '+' => stats.add_lines(&author_email, 1),
                            '-' => stats.delete_lines(&author_email, 1),
                            _ => {}
                        }
                    }
                }
                true
            }),
        ).context("Failed to process diff lines")?;

        Ok(CommitRecord {
            sha: oid.to_string(),
            author_name,
            author_email,
            message,
            authored_at: commit_time,
            files_changed_count: files_changed.len(),
            changed_files: files_changed,
        })
    }

    /// Get the latest commit for a specific file
    pub fn get_file_last_commit(&self, file_path: &str) -> Result<Option<DateTime<Utc>>> {
        let mut revwalk = self.repo.revwalk()
            .context("Failed to create revwalk")?;
        
        revwalk.push_head()
            .context("Failed to push HEAD")?;
        
        for oid in revwalk {
            let oid = oid.context("Failed to get commit OID")?;
            let commit = self.repo.find_commit(oid)
                .context("Failed to find commit")?;
            
            let tree = commit.tree().context("Failed to get tree")?;
            
            // Check if file exists in this commit
            if tree.get_path(Path::new(file_path)).is_ok() {
                let commit_time = Utc.timestamp_opt(commit.time().seconds(), 0)
                    .single()
                    .unwrap_or_else(Utc::now);
                return Ok(Some(commit_time));
            }
        }
        
        Ok(None)
    }
}

/// Internal file statistics tracker
#[derive(Debug, Clone)]
struct FileStats {
    commit_count: usize,
    last_modified: DateTime<Utc>,
    contributors: HashMap<String, ContributorStats>,
}

#[derive(Debug, Clone)]
struct ContributorStats {
    name: String,
    email: String,
    commit_count: usize,
    lines_added: usize,
    lines_deleted: usize,
}

impl FileStats {
    fn new() -> Self {
        Self {
            commit_count: 0,
            last_modified: Utc.timestamp_opt(0, 0).single().unwrap(),
            contributors: HashMap::new(),
        }
    }

    fn update_from_commit(
        &mut self,
        author_email: &str,
        author_name: &str,
        commit_time: DateTime<Utc>,
    ) {
        self.commit_count += 1;
        
        if commit_time > self.last_modified {
            self.last_modified = commit_time;
        }

        let stats = self.contributors.entry(author_email.to_string())
            .or_insert_with(|| ContributorStats {
                name: author_name.to_string(),
                email: author_email.to_string(),
                commit_count: 0,
                lines_added: 0,
                lines_deleted: 0,
            });
        
        stats.commit_count += 1;
    }

    fn add_lines(&mut self, author_email: &str, count: usize) {
        if let Some(stats) = self.contributors.get_mut(author_email) {
            stats.lines_added += count;
        }
    }

    fn delete_lines(&mut self, author_email: &str, count: usize) {
        if let Some(stats) = self.contributors.get_mut(author_email) {
            stats.lines_deleted += count;
        }
    }

    fn to_contribution(self, path: String) -> FileContribution {
        let mut contributors: Vec<ContributorInfo> = self.contributors
            .into_iter()
            .map(|(email, stats)| ContributorInfo {
                email,
                name: stats.name,
                commit_count: stats.commit_count,
                lines_added: stats.lines_added,
                lines_deleted: stats.lines_deleted,
            })
            .collect();

        // Sort by commit count descending
        contributors.sort_by(|a, b| b.commit_count.cmp(&a.commit_count));

        let primary_author = contributors.first()
            .map(|c| c.email.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let lines_added_total: usize = contributors.iter().map(|c| c.lines_added).sum();
        let lines_deleted_total: usize = contributors.iter().map(|c| c.lines_deleted).sum();
        let lines_changed_total = lines_added_total + lines_deleted_total;

        FileContribution {
            file_path: path,
            commit_count: self.commit_count,
            last_modified: self.last_modified,
            primary_author,
            contributors,
            lines_added_total,
            lines_deleted_total,
            lines_changed_total,
        }
    }
}

/// Check if a file is a code file that should be analyzed
fn is_code_file(path: &str) -> bool {
    let code_extensions = [
        ".rs", ".go", ".py", ".js", ".ts", ".tsx", ".jsx",
        ".java", ".c", ".cpp", ".h", ".hpp", ".cs",
        ".rb", ".php", ".swift", ".kt", ".scala",
    ];

    code_extensions.iter().any(|ext| path.ends_with(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_code_file() {
        assert!(is_code_file("src/main.rs"));
        assert!(is_code_file("app.js"));
        assert!(is_code_file("component.tsx"));
        assert!(!is_code_file("README.md"));
        assert!(!is_code_file("package.json"));
        assert!(!is_code_file(".gitignore"));
    }
}
