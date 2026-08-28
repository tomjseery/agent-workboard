use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorktree {
    pub path: PathBuf,
    pub common_dir: PathBuf,
    pub git_dir: PathBuf,
    pub branch: Option<String>,
    pub head_oid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredRepository {
    pub common_dir: PathBuf,
    pub worktrees: Vec<DiscoveredWorktree>,
    pub branches: Vec<DiscoveredBranch>,
    pub remotes: Vec<DiscoveredRemote>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredWorktree {
    pub path: PathBuf,
    pub git_dir: Option<PathBuf>,
    pub branch: Option<String>,
    pub head_oid: String,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredBranch {
    pub full_name: String,
    pub oid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredRemote {
    pub name: String,
    pub url: String,
}

pub trait GitWorktreeResolver {
    fn resolve(&self, path: &Path) -> Result<ResolvedWorktree, AppError>;
}

pub trait GitRepositoryDiscovery {
    fn discover(&self, path: &Path) -> Result<DiscoveredRepository, AppError>;
}

pub trait GitWorktreeCreator {
    fn recreate(
        &self,
        repository: &Path,
        target: &Path,
        branch: &str,
        create_branch: bool,
        start_point: &str,
    ) -> Result<ResolvedWorktree, AppError>;
}

pub struct GitCli;

impl GitCli {
    pub fn restore_missing_worktree(
        &self,
        repository: &Path,
        target: &Path,
        branch: &str,
    ) -> Result<ResolvedWorktree, AppError> {
        if !target.is_absolute() {
            return Err(AppError::WorktreePathNotAbsolute(target.to_owned()));
        }
        if target.exists() {
            return Err(AppError::RecreateCheckoutPathExists(target.to_owned()));
        }
        let discovered = self.discover(repository)?;
        let full_branch = format!("refs/heads/{branch}");
        let missing_registration = discovered.worktrees.iter().any(|worktree| {
            !worktree.present && worktree.branch.as_deref() == Some(full_branch.as_str())
        });
        if !missing_registration {
            return self.recreate(repository, target, branch, false, branch);
        }
        if discovered.worktrees.iter().any(|worktree| {
            worktree.present
                && worktree.branch.as_deref() == Some(full_branch.as_str())
                && !paths_equal(&worktree.path, target)
        }) {
            return Err(AppError::GitCommand {
                message: format!("branch {branch} is already checked out elsewhere"),
            });
        }
        let parent = target
            .parent()
            .filter(|value| value.is_dir())
            .ok_or_else(|| AppError::RecreateCheckoutParentMissing(target.to_owned()))?;
        let file_name = target
            .file_name()
            .ok_or_else(|| AppError::RecreateCheckoutParentMissing(target.to_owned()))?;
        let target = git_compatible_path(
            &parent
                .canonicalize()
                .map_err(AppError::GitIo)?
                .join(file_name),
        );
        successful_text(run_git(
            repository,
            &["check-ref-format", "--branch", branch],
        )?)?;
        let output = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(["worktree", "add", "--force", "--"])
            .arg(&target)
            .arg(branch)
            .output()
            .map_err(AppError::GitIo)?;
        successful_text(output)?;
        self.resolve(&target)
    }
}

impl GitWorktreeCreator for GitCli {
    fn recreate(
        &self,
        repository: &Path,
        target: &Path,
        branch: &str,
        create_branch: bool,
        start_point: &str,
    ) -> Result<ResolvedWorktree, AppError> {
        if !target.is_absolute() {
            return Err(AppError::WorktreePathNotAbsolute(target.to_owned()));
        }
        if target.exists() {
            return Err(AppError::RecreateCheckoutPathExists(target.to_owned()));
        }
        let parent = target
            .parent()
            .filter(|value| value.is_dir())
            .ok_or_else(|| AppError::RecreateCheckoutParentMissing(target.to_owned()))?;
        let file_name = target
            .file_name()
            .ok_or_else(|| AppError::RecreateCheckoutParentMissing(target.to_owned()))?;
        let target = git_compatible_path(
            &parent
                .canonicalize()
                .map_err(AppError::GitIo)?
                .join(file_name),
        );
        if branch.trim().is_empty() || start_point.trim().is_empty() {
            return Err(AppError::GitCommand {
                message: "branch and start point cannot be blank".to_owned(),
            });
        }
        successful_text(run_git(
            repository,
            &["check-ref-format", "--branch", branch],
        )?)?;

        let mut command = Command::new("git");
        command.arg("-C").arg(repository).args(["worktree", "add"]);
        if create_branch {
            command.arg("-b").arg(branch);
        }
        command.arg("--").arg(&target);
        if create_branch {
            command.arg(start_point);
        } else {
            command.arg(branch);
        }
        successful_text(command.output().map_err(AppError::GitIo)?)?;
        self.resolve(&target)
    }
}

impl GitWorktreeResolver for GitCli {
    fn resolve(&self, path: &Path) -> Result<ResolvedWorktree, AppError> {
        if !path.is_absolute() {
            return Err(AppError::WorktreePathNotAbsolute(path.to_path_buf()));
        }
        if !path.is_dir() {
            return Err(AppError::WorktreePathInvalid(path.to_path_buf()));
        }

        let requested_path = path
            .canonicalize()
            .map_err(|_| AppError::WorktreePathInvalid(path.to_path_buf()))?;
        let root = git_path(
            &requested_path,
            &["rev-parse", "--path-format=absolute", "--show-toplevel"],
        )?;
        let root = root
            .canonicalize()
            .map_err(|_| AppError::WorktreePathInvalid(root.clone()))?;

        if !paths_equal(&requested_path, &root) {
            return Err(AppError::WorktreePathNotRoot(requested_path));
        }

        let common_dir = git_path(
            &root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?
        .canonicalize()
        .map_err(|_| AppError::WorktreeNotRegistered(root.clone()))?;
        let git_dir = git_path(
            &root,
            &["rev-parse", "--path-format=absolute", "--absolute-git-dir"],
        )?
        .canonicalize()
        .map_err(|_| AppError::WorktreeNotRegistered(root.clone()))?;

        validate_registered_worktree(&root)?;

        let branch = symbolic_branch(&root)?;
        let head_oid = git_text(&root, &["rev-parse", "--verify", "HEAD"])?;

        Ok(ResolvedWorktree {
            path: root,
            common_dir,
            git_dir,
            branch,
            head_oid,
        })
    }
}

impl GitRepositoryDiscovery for GitCli {
    fn discover(&self, path: &Path) -> Result<DiscoveredRepository, AppError> {
        let resolved = self.resolve(path)?;
        let output = run_git(&resolved.path, &["worktree", "list", "--porcelain", "-z"])?;
        let text = successful_text(output)?;
        let mut worktrees = parse_worktrees(&text);
        for worktree in &mut worktrees {
            worktree.present = worktree.path.is_dir();
            if worktree.present {
                worktree.git_dir = git_path(
                    &worktree.path,
                    &["rev-parse", "--path-format=absolute", "--absolute-git-dir"],
                )?
                .canonicalize()
                .ok();
            }
        }

        let branches = git_text(
            &resolved.path,
            &[
                "for-each-ref",
                "--format=%(refname)%09%(objectname)",
                "refs/heads",
            ],
        )?
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(full_name, oid)| DiscoveredBranch {
            full_name: full_name.to_owned(),
            oid: oid.to_owned(),
        })
        .collect();
        let remote_names = git_text(&resolved.path, &["remote"])?;
        let mut remotes = Vec::new();
        for name in remote_names.lines().filter(|name| !name.is_empty()) {
            let urls = git_text(&resolved.path, &["remote", "get-url", "--all", name])?;
            remotes.extend(urls.lines().filter(|url| !url.is_empty()).map(|url| {
                DiscoveredRemote {
                    name: name.to_owned(),
                    url: redact_remote_url(url),
                }
            }));
        }

        Ok(DiscoveredRepository {
            common_dir: resolved.common_dir,
            worktrees,
            branches,
            remotes,
        })
    }
}

impl ResolvedWorktree {
    pub fn path_text(&self) -> Result<&str, AppError> {
        path_text(&self.path)
    }

    pub fn common_dir_text(&self) -> Result<&str, AppError> {
        path_text(&self.common_dir)
    }

    pub fn git_dir_text(&self) -> Result<&str, AppError> {
        path_text(&self.git_dir)
    }
}

fn git_path(cwd: &Path, arguments: &[&str]) -> Result<PathBuf, AppError> {
    git_text(cwd, arguments).map(PathBuf::from)
}

fn git_text(cwd: &Path, arguments: &[&str]) -> Result<String, AppError> {
    let output = run_git(cwd, arguments)?;
    successful_text(output)
}

fn run_git(cwd: &Path, arguments: &[&str]) -> Result<Output, AppError> {
    Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(arguments)
        .output()
        .map_err(AppError::GitIo)
}

fn successful_text(output: Output) -> Result<String, AppError> {
    if !output.status.success() {
        return Err(git_command_error(output));
    }

    String::from_utf8(output.stdout)
        .map(|value| value.trim_end_matches(['\r', '\n']).to_owned())
        .map_err(|_| AppError::GitOutputEncoding)
}

fn git_command_error(output: Output) -> AppError {
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    AppError::GitCommand {
        message: if message.is_empty() {
            format!("Git exited with {}", output.status)
        } else {
            message
        },
    }
}

fn symbolic_branch(root: &Path) -> Result<Option<String>, AppError> {
    let output = run_git(root, &["symbolic-ref", "--quiet", "HEAD"])?;
    if output.status.success() {
        return String::from_utf8(output.stdout)
            .map(|value| Some(value.trim_end_matches(['\r', '\n']).to_owned()))
            .map_err(|_| AppError::GitOutputEncoding);
    }
    if output.status.code() == Some(1) {
        return Ok(None);
    }

    Err(git_command_error(output))
}

fn validate_registered_worktree(root: &Path) -> Result<(), AppError> {
    let output = run_git(root, &["worktree", "list", "--porcelain", "-z"])?;
    if !output.status.success() {
        return Err(git_command_error(output));
    }
    let text = String::from_utf8(output.stdout).map_err(|_| AppError::GitOutputEncoding)?;

    let registered = text.split('\0').any(|field| {
        field
            .strip_prefix("worktree ")
            .and_then(|value| Path::new(value).canonicalize().ok())
            .is_some_and(|candidate| paths_equal(&candidate, root))
    });

    if registered {
        Ok(())
    } else {
        Err(AppError::WorktreeNotRegistered(root.to_path_buf()))
    }
}

fn parse_worktrees(text: &str) -> Vec<DiscoveredWorktree> {
    let mut records = Vec::new();
    let mut current: Option<DiscoveredWorktree> = None;
    for field in text.split('\0').filter(|field| !field.is_empty()) {
        if let Some(path) = field.strip_prefix("worktree ") {
            if let Some(record) = current.take() {
                records.push(record);
            }
            current = Some(DiscoveredWorktree {
                path: PathBuf::from(path),
                git_dir: None,
                branch: None,
                head_oid: String::new(),
                present: false,
            });
        } else if let Some(record) = current.as_mut() {
            if let Some(oid) = field.strip_prefix("HEAD ") {
                record.head_oid = oid.to_owned();
            } else if let Some(branch) = field.strip_prefix("branch ") {
                record.branch = Some(branch.to_owned());
            }
        }
    }
    if let Some(record) = current {
        records.push(record);
    }
    records
}

fn redact_remote_url(value: &str) -> String {
    let without_suffix = value
        .split_once(['?', '#'])
        .map_or(value, |(prefix, _)| prefix);
    if let Some((scheme, remainder)) = without_suffix.split_once("://") {
        let authority_end = remainder.find('/').unwrap_or(remainder.len());
        let (authority, path) = remainder.split_at(authority_end);
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        return format!("{scheme}://{host}{path}");
    }
    if let Some((_, host_path)) = without_suffix.split_once('@')
        && host_path.contains(':')
    {
        return host_path.to_owned();
    }
    without_suffix.to_owned()
}

fn path_text(path: &Path) -> Result<&str, AppError> {
    path.to_str()
        .ok_or_else(|| AppError::GitPathEncoding(path.to_path_buf()))
}

#[cfg(windows)]
fn git_compatible_path(path: &Path) -> PathBuf {
    let value = path.as_os_str().to_string_lossy();
    if let Some(value) = value.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{value}"))
    } else if let Some(value) = value.strip_prefix(r"\\?\") {
        PathBuf::from(value)
    } else {
        path.to_owned()
    }
}

#[cfg(not(windows))]
fn git_compatible_path(path: &Path) -> PathBuf {
    path.to_owned()
}

#[cfg(windows)]
fn paths_equal(left: &Path, right: &Path) -> bool {
    windows_path_text(left).eq_ignore_ascii_case(&windows_path_text(right))
}

#[cfg(windows)]
fn windows_path_text(path: &Path) -> String {
    let value = path.as_os_str().to_string_lossy().replace('/', "\\");
    value
        .strip_prefix(r"\\?\")
        .unwrap_or(&value)
        .trim_end_matches('\\')
        .to_owned()
}

#[cfg(not(windows))]
fn paths_equal(left: &Path, right: &Path) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use super::{parse_worktrees, redact_remote_url};

    #[test]
    fn parses_porcelain_worktrees_with_detached_head() {
        let records = parse_worktrees(concat!(
            "worktree C:/fixture/main\0HEAD abc\0branch refs/heads/main\0\0",
            "worktree C:/fixture/detached\0HEAD def\0detached\0\0"
        ));

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].branch.as_deref(), Some("refs/heads/main"));
        assert_eq!(records[1].branch, None);
        assert_eq!(records[1].head_oid, "def");
    }

    #[test]
    fn removes_remote_credentials_and_query_values() {
        assert_eq!(
            redact_remote_url("https://token@example.invalid/team/repo.git?secret=yes"),
            "https://example.invalid/team/repo.git"
        );
        assert_eq!(
            redact_remote_url("git@example.invalid:team/repo.git"),
            "example.invalid:team/repo.git"
        );
    }
}
