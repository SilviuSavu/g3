//! Lightweight async git worktree helpers for autonomous mode isolation.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Find the git repo root from a path.
pub async fn find_git_root(from: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["-C", &from.to_string_lossy(), "rev-parse", "--show-toplevel"])
        .output()
        .await
        .context("failed to run git rev-parse")?;

    if !output.status.success() {
        anyhow::bail!(
            "not a git repository: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let root = String::from_utf8(output.stdout)
        .context("invalid utf-8 in git output")?
        .trim()
        .to_string();

    Ok(PathBuf::from(root))
}

/// Create a worktree at `.worktrees/autonomous/<branch_name>` branching from HEAD.
/// Returns the absolute path to the new worktree.
pub async fn create_worktree(repo_root: &Path, branch_name: &str) -> Result<PathBuf> {
    let worktree_dir = repo_root.join(".worktrees").join("autonomous");
    tokio::fs::create_dir_all(&worktree_dir)
        .await
        .context("failed to create .worktrees/autonomous/")?;

    let wt_path = worktree_dir.join(branch_name);

    let output = Command::new("git")
        .args([
            "-C",
            &repo_root.to_string_lossy(),
            "worktree",
            "add",
            "-b",
            branch_name,
            &wt_path.to_string_lossy(),
        ])
        .output()
        .await
        .context("failed to run git worktree add")?;

    if !output.status.success() {
        anyhow::bail!(
            "git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(wt_path)
}

/// Commit all changes in the worktree (git add -A + commit).
/// Returns true if anything was committed, false if working tree was clean.
pub async fn commit_all(worktree_path: &Path, message: &str) -> Result<bool> {
    // Stage everything
    let output = Command::new("git")
        .args(["-C", &worktree_path.to_string_lossy(), "add", "-A"])
        .output()
        .await
        .context("failed to run git add")?;

    if !output.status.success() {
        anyhow::bail!(
            "git add -A failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    // Check if there are staged changes
    let diff = Command::new("git")
        .args([
            "-C",
            &worktree_path.to_string_lossy(),
            "diff",
            "--cached",
            "--quiet",
        ])
        .output()
        .await
        .context("failed to run git diff --cached")?;

    // exit 0 = clean, exit 1 = changes exist
    if diff.status.success() {
        return Ok(false);
    }

    // Commit
    let output = Command::new("git")
        .args([
            "-C",
            &worktree_path.to_string_lossy(),
            "commit",
            "-m",
            message,
        ])
        .output()
        .await
        .context("failed to run git commit")?;

    if !output.status.success() {
        anyhow::bail!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(true)
}

/// Merge worktree branch into the current branch of the main worktree.
pub async fn merge_to_main(repo_root: &Path, branch_name: &str) -> Result<()> {
    let output = Command::new("git")
        .args([
            "-C",
            &repo_root.to_string_lossy(),
            "merge",
            branch_name,
            "-m",
            &format!("autonomous: merge {}", branch_name),
        ])
        .output()
        .await
        .context("failed to run git merge")?;

    if !output.status.success() {
        anyhow::bail!(
            "git merge failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(())
}

/// Remove the worktree and delete its branch.
pub async fn remove_worktree(
    repo_root: &Path,
    worktree_path: &Path,
    branch_name: &str,
) -> Result<()> {
    // Remove worktree
    let _ = Command::new("git")
        .args([
            "-C",
            &repo_root.to_string_lossy(),
            "worktree",
            "remove",
            "--force",
            &worktree_path.to_string_lossy(),
        ])
        .output()
        .await;

    // Prune stale worktree metadata
    let _ = Command::new("git")
        .args(["-C", &repo_root.to_string_lossy(), "worktree", "prune"])
        .output()
        .await;

    // Delete the branch
    let _ = Command::new("git")
        .args([
            "-C",
            &repo_root.to_string_lossy(),
            "branch",
            "-D",
            branch_name,
        ])
        .output()
        .await;

    Ok(())
}

/// Generate a short unique branch name for an autonomous session.
/// Format: `autonomous/<8-hex-chars>`
pub fn autonomous_branch_name() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let hex: String = (0..8).map(|_| format!("{:x}", rng.gen::<u8>() % 16)).collect();
    format!("autonomous/{}", hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autonomous_branch_name_format() {
        let name = autonomous_branch_name();
        assert!(name.starts_with("autonomous/"));
        assert_eq!(name.len(), "autonomous/".len() + 8);
        // All chars after prefix should be hex
        for c in name["autonomous/".len()..].chars() {
            assert!(c.is_ascii_hexdigit());
        }
    }

    #[test]
    fn test_autonomous_branch_name_unique() {
        let a = autonomous_branch_name();
        let b = autonomous_branch_name();
        // Extremely unlikely to collide with 8 hex chars
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn test_create_and_remove_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();

        // Init a git repo with an initial commit
        Command::new("git")
            .args(["init", &repo.to_string_lossy()])
            .output()
            .await
            .unwrap();
        Command::new("git")
            .args(["-C", &repo.to_string_lossy(), "config", "user.email", "test@test.com"])
            .output()
            .await
            .unwrap();
        Command::new("git")
            .args(["-C", &repo.to_string_lossy(), "config", "user.name", "Test"])
            .output()
            .await
            .unwrap();
        tokio::fs::write(repo.join("README.md"), "hello").await.unwrap();
        Command::new("git")
            .args(["-C", &repo.to_string_lossy(), "add", "-A"])
            .output()
            .await
            .unwrap();
        Command::new("git")
            .args(["-C", &repo.to_string_lossy(), "commit", "-m", "init"])
            .output()
            .await
            .unwrap();

        // Create worktree
        let branch = "autonomous/test1234";
        let wt = create_worktree(repo, branch).await.unwrap();
        assert!(wt.exists());

        // Verify find_git_root from the main repo works
        let root = find_git_root(repo).await.unwrap();
        assert_eq!(
            root.canonicalize().unwrap(),
            repo.canonicalize().unwrap()
        );

        // commit_all on clean worktree should return false
        let committed = commit_all(&wt, "test").await.unwrap();
        assert!(!committed);

        // Write a file and commit
        tokio::fs::write(wt.join("new.txt"), "data").await.unwrap();
        let committed = commit_all(&wt, "add new.txt").await.unwrap();
        assert!(committed);

        // Merge to main
        merge_to_main(repo, branch).await.unwrap();
        assert!(repo.join("new.txt").exists());

        // Cleanup
        remove_worktree(repo, &wt, branch).await.unwrap();
        assert!(!wt.exists());
    }
}
