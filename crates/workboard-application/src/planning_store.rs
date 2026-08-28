use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use workboard_core::{DocumentId, DocumentKind, Slug, WorkItemStatus};

use crate::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentFrontMatter {
    pub id: DocumentId,
    pub kind: DocumentKind,
    pub key: String,
    pub status: Option<WorkItemStatus>,
    pub repositories: Vec<Slug>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredDocument {
    pub front_matter: DocumentFrontMatter,
    pub body: String,
    pub relative_path: PathBuf,
    pub content_hash: String,
    pub observed_commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPlanningDocument {
    pub relative_path: PathBuf,
    pub front_matter: DocumentFrontMatter,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanningStore {
    root: PathBuf,
}

impl PlanningStore {
    pub fn create_or_link(path: &Path) -> Result<Self, AppError> {
        if !path.is_absolute() {
            return Err(AppError::PlanningStoreInvalid(path.to_path_buf()));
        }
        if path.exists() && !path.is_dir() {
            return Err(AppError::PlanningStoreInvalid(path.to_path_buf()));
        }
        if !path.exists() {
            fs::create_dir_all(path)
                .map_err(|source| planning_io("creating the store", path, source))?;
        }
        let root = path
            .canonicalize()
            .map_err(|source| planning_io("resolving the store", path, source))?;
        let git_directory = root.join(".git");
        if !git_directory.exists() {
            let mut entries = fs::read_dir(&root)
                .map_err(|source| planning_io("reading the store", &root, source))?;
            if entries
                .next()
                .transpose()
                .map_err(|source| planning_io("reading the store", &root, source))?
                .is_some()
            {
                return Err(AppError::PlanningStoreInvalid(root));
            }
            successful_git(
                Command::new("git")
                    .arg("init")
                    .args(["-b", "main"])
                    .arg(&root)
                    .output()
                    .map_err(AppError::GitIo)?,
            )?;
        }
        let reported_root = git_text(
            &root,
            ["rev-parse", "--path-format=absolute", "--show-toplevel"],
        )?;
        let reported_root = PathBuf::from(reported_root)
            .canonicalize()
            .map_err(|source| planning_io("resolving the Git root", &root, source))?;
        if !paths_equal(&root, &reported_root) {
            return Err(AppError::PlanningStoreInvalid(root));
        }
        configure_platform_git(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn git_directory(&self) -> Result<PathBuf, AppError> {
        let value = git_text(
            &self.root,
            ["rev-parse", "--path-format=absolute", "--absolute-git-dir"],
        )?;
        Ok(PathBuf::from(value))
    }

    pub fn workspace_config_path(workspace: &Slug) -> PathBuf {
        PathBuf::from("workspaces")
            .join(workspace.as_str())
            .join("workspace.toml")
    }

    pub fn epic_path(workspace: &Slug, epic: &Slug) -> PathBuf {
        PathBuf::from("workspaces")
            .join(workspace.as_str())
            .join("epics")
            .join(epic.as_str())
            .join("EPIC.md")
    }

    pub fn feature_path(workspace: &Slug, epic: &Slug, feature: &Slug) -> PathBuf {
        PathBuf::from("workspaces")
            .join(workspace.as_str())
            .join("epics")
            .join(epic.as_str())
            .join("features")
            .join(feature.as_str())
            .join("FEATURE.md")
    }

    pub fn work_item_path(
        workspace: &Slug,
        epic: &Slug,
        feature: &Slug,
        work_item: &Slug,
    ) -> PathBuf {
        PathBuf::from("workspaces")
            .join(workspace.as_str())
            .join("epics")
            .join(epic.as_str())
            .join("features")
            .join(feature.as_str())
            .join("work-items")
            .join(format!("{}.md", work_item.as_str()))
    }

    pub fn initialise_workspace(&self, workspace: &Slug, title: &str) -> Result<PathBuf, AppError> {
        if title.trim().is_empty() {
            return Err(AppError::Domain(
                "workspace title cannot be blank".to_owned(),
            ));
        }
        let path = Self::workspace_config_path(workspace);
        let content = format!(
            "version = 1\nslug = {}\ntitle = {}\n",
            toml_string(workspace.as_str()),
            toml_string(title)
        );
        self.write_new_bytes(&path, content.as_bytes())?;
        Ok(path)
    }

    pub fn publish_new(
        &self,
        relative_path: &Path,
        front_matter: &DocumentFrontMatter,
        body: &str,
        message: &str,
    ) -> Result<StoredDocument, AppError> {
        let bytes = render_document(front_matter, body)?;
        self.write_new_bytes(relative_path, bytes.as_bytes())?;
        let observed_commit = self.commit_paths([relative_path], message)?;
        Ok(StoredDocument {
            front_matter: front_matter.clone(),
            body: normalise_body(body),
            relative_path: relative_path.to_path_buf(),
            content_hash: content_hash(bytes.as_bytes()),
            observed_commit: Some(observed_commit),
        })
    }

    pub fn publish_update(
        &self,
        relative_path: &Path,
        expected_hash: &str,
        front_matter: &DocumentFrontMatter,
        body: &str,
        message: &str,
    ) -> Result<StoredDocument, AppError> {
        let path = self.resolve_relative(relative_path)?;
        let existing =
            fs::read(&path).map_err(|source| planning_io("reading a document", &path, source))?;
        if content_hash(&existing) != expected_hash {
            return Err(AppError::PlanningDocumentConcurrentEdit(path));
        }
        let bytes = render_document(front_matter, body)?;
        let temporary = path.with_extension(format!("workboard-{}.tmp", DocumentId::generate()));
        fs::write(&temporary, bytes.as_bytes())
            .map_err(|source| planning_io("writing a document candidate", &temporary, source))?;
        let current = fs::read(&path)
            .map_err(|source| planning_io("rechecking a document", &path, source))?;
        if content_hash(&current) != expected_hash {
            drop(fs::remove_file(&temporary));
            return Err(AppError::PlanningDocumentConcurrentEdit(path));
        }
        fs::copy(&temporary, &path)
            .map_err(|source| planning_io("publishing a document", &path, source))?;
        fs::remove_file(&temporary)
            .map_err(|source| planning_io("removing a document candidate", &temporary, source))?;
        let observed_commit = self.commit_paths([relative_path], message)?;
        Ok(StoredDocument {
            front_matter: front_matter.clone(),
            body: normalise_body(body),
            relative_path: relative_path.to_path_buf(),
            content_hash: content_hash(bytes.as_bytes()),
            observed_commit: Some(observed_commit),
        })
    }

    pub fn publish_batch_new(
        &self,
        documents: &[NewPlanningDocument],
        message: &str,
    ) -> Result<Vec<StoredDocument>, AppError> {
        if documents.is_empty() {
            return Err(AppError::PlanningDocumentInvalid(
                "at least one planning document is required".to_owned(),
            ));
        }
        let mut candidates = Vec::with_capacity(documents.len());
        for document in documents {
            let path = self.resolve_relative(&document.relative_path)?;
            if candidates.iter().any(
                |(candidate, _, _): &(PathBuf, String, &NewPlanningDocument)| {
                    paths_equal(candidate, &path)
                },
            ) {
                return Err(AppError::PlanningDocumentInvalid(
                    "planning publication contains a duplicate path".to_owned(),
                ));
            }
            let rendered = render_document(&document.front_matter, &document.body)?;
            if path.is_file() {
                let existing = fs::read(&path)
                    .map_err(|source| planning_io("reading a document", &path, source))?;
                if existing != rendered.as_bytes() {
                    return Err(AppError::PlanningDocumentExists(path));
                }
            }
            candidates.push((path, rendered, document));
        }
        for (path, rendered, _) in &candidates {
            if path.exists() {
                continue;
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|source| {
                    planning_io("creating document directories", parent, source)
                })?;
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|source| planning_io("creating a document", path, source))?;
            file.write_all(rendered.as_bytes())
                .and_then(|_| file.sync_all())
                .map_err(|source| planning_io("writing a document", path, source))?;
        }
        let relative_paths = documents
            .iter()
            .map(|document| document.relative_path.as_path())
            .collect::<Vec<_>>();
        let changed = relative_paths
            .iter()
            .map(|path| self.path_is_changed(path))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .any(|changed| changed);
        let observed_commit = if changed {
            self.commit_paths(relative_paths, message)?
        } else {
            self.head()?
        };
        Ok(candidates
            .into_iter()
            .map(|(_, rendered, document)| StoredDocument {
                front_matter: document.front_matter.clone(),
                body: normalise_body(&document.body),
                relative_path: document.relative_path.clone(),
                content_hash: content_hash(rendered.as_bytes()),
                observed_commit: Some(observed_commit.clone()),
            })
            .collect())
    }

    pub fn read_document(&self, relative_path: &Path) -> Result<StoredDocument, AppError> {
        let path = self.resolve_relative(relative_path)?;
        let bytes =
            fs::read(&path).map_err(|source| planning_io("reading a document", &path, source))?;
        let text = String::from_utf8(bytes.clone()).map_err(|_| {
            AppError::PlanningDocumentInvalid(format!("{} is not UTF-8", relative_path.display()))
        })?;
        let (front_matter, body) = parse_document(&text)?;
        Ok(StoredDocument {
            front_matter,
            body,
            relative_path: relative_path.to_path_buf(),
            content_hash: content_hash(&bytes),
            observed_commit: self.head().ok(),
        })
    }

    pub fn commit_paths<'a>(
        &self,
        paths: impl IntoIterator<Item = &'a Path>,
        message: &str,
    ) -> Result<String, AppError> {
        if message.trim().is_empty() {
            return Err(AppError::PlanningDocumentInvalid(
                "commit message cannot be blank".to_owned(),
            ));
        }
        let paths: Vec<&Path> = paths.into_iter().collect();
        if paths.is_empty() {
            return Err(AppError::PlanningDocumentInvalid(
                "at least one commit path is required".to_owned(),
            ));
        }
        let mut add = Command::new("git");
        add.arg("-C").arg(&self.root).args(["add", "--"]);
        for path in &paths {
            self.resolve_relative(path)?;
            add.arg(path);
        }
        successful_git(add.output().map_err(AppError::GitIo)?)?;
        let mut commit = Command::new("git");
        commit
            .arg("-C")
            .arg(&self.root)
            .args(["commit", "-m", message]);
        successful_git(commit.output().map_err(AppError::GitIo)?)?;
        self.head()
    }

    pub fn head(&self) -> Result<String, AppError> {
        git_text(&self.root, ["rev-parse", "--verify", "HEAD"])
    }

    pub fn export(&self, destination: &Path) -> Result<(), AppError> {
        if !destination.is_absolute() || destination.exists() {
            return Err(AppError::PlanningStoreInvalid(destination.to_path_buf()));
        }
        fs::create_dir_all(destination)
            .map_err(|source| planning_io("creating an export", destination, source))?;
        copy_store(&self.root, destination)
    }

    fn write_new_bytes(&self, relative_path: &Path, bytes: &[u8]) -> Result<(), AppError> {
        let path = self.resolve_relative(relative_path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| planning_io("creating document directories", parent, source))?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| {
                if source.kind() == std::io::ErrorKind::AlreadyExists {
                    AppError::PlanningDocumentExists(path.clone())
                } else {
                    planning_io("creating a document", &path, source)
                }
            })?;
        file.write_all(bytes)
            .map_err(|source| planning_io("writing a document", &path, source))?;
        file.sync_all()
            .map_err(|source| planning_io("flushing a document", &path, source))?;
        Ok(())
    }

    fn resolve_relative(&self, relative_path: &Path) -> Result<PathBuf, AppError> {
        if relative_path.as_os_str().is_empty()
            || relative_path.is_absolute()
            || relative_path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(AppError::PlanningDocumentInvalid(format!(
                "unsafe relative path: {}",
                relative_path.display()
            )));
        }
        Ok(self.root.join(relative_path))
    }

    fn path_is_changed(&self, relative_path: &Path) -> Result<bool, AppError> {
        self.resolve_relative(relative_path)?;
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["status", "--porcelain", "--"])
            .arg(relative_path)
            .output()
            .map_err(AppError::GitIo)?;
        successful_git(output).map(|value| !value.trim().is_empty())
    }
}

#[cfg(windows)]
fn configure_platform_git(root: &Path) -> Result<(), AppError> {
    successful_git(
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["config", "core.longpaths", "true"])
            .output()
            .map_err(AppError::GitIo)?,
    )?;
    Ok(())
}

#[cfg(not(windows))]
fn configure_platform_git(_root: &Path) -> Result<(), AppError> {
    Ok(())
}

fn render_document(front_matter: &DocumentFrontMatter, body: &str) -> Result<String, AppError> {
    if front_matter.key.trim().is_empty()
        || front_matter.key.contains(['\r', '\n'])
        || body.trim().is_empty()
    {
        return Err(AppError::PlanningDocumentInvalid(
            "front matter key and document body are required".to_owned(),
        ));
    }
    let kind = document_kind(front_matter.kind)?;
    let mut rendered = format!(
        "---\nid: {}\nkind: {kind}\nkey: {}\n",
        front_matter.id, front_matter.key
    );
    if let Some(status) = front_matter.status {
        rendered.push_str(&format!("status: {}\n", work_item_status(status)));
    }
    rendered.push_str("repositories:\n");
    for repository in &front_matter.repositories {
        rendered.push_str(&format!("  - {}\n", repository.as_str()));
    }
    rendered.push_str("---\n\n");
    rendered.push_str(&normalise_body(body));
    Ok(rendered)
}

fn parse_document(value: &str) -> Result<(DocumentFrontMatter, String), AppError> {
    let value = value.replace("\r\n", "\n");
    let remainder = value.strip_prefix("---\n").ok_or_else(|| {
        AppError::PlanningDocumentInvalid("front matter opening delimiter is missing".to_owned())
    })?;
    let (header, body) = remainder.split_once("\n---\n").ok_or_else(|| {
        AppError::PlanningDocumentInvalid("front matter closing delimiter is missing".to_owned())
    })?;
    let mut id = None;
    let mut kind = None;
    let mut key = None;
    let mut status = None;
    let mut repositories = Vec::new();
    let mut reading_repositories = false;
    for line in header.lines() {
        if let Some(repository) = line.strip_prefix("  - ") {
            if !reading_repositories {
                return Err(AppError::PlanningDocumentInvalid(
                    "repository entry is outside its list".to_owned(),
                ));
            }
            repositories.push(
                Slug::new(repository)
                    .map_err(|error| AppError::PlanningDocumentInvalid(error.to_string()))?,
            );
            continue;
        }
        reading_repositories = false;
        let (name, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match name {
            "id" => {
                id = Some(value.parse().map_err(|_| {
                    AppError::PlanningDocumentInvalid("document ID is invalid".to_owned())
                })?)
            }
            "kind" => kind = Some(parse_document_kind(value)?),
            "key" if !value.is_empty() => key = Some(value.to_owned()),
            "status" => status = Some(parse_work_item_status(value)?),
            "repositories" if value.is_empty() => reading_repositories = true,
            _ => {
                return Err(AppError::PlanningDocumentInvalid(format!(
                    "unsupported front matter field: {name}"
                )));
            }
        }
    }
    let front_matter = DocumentFrontMatter {
        id: id.ok_or_else(|| {
            AppError::PlanningDocumentInvalid("document ID is missing".to_owned())
        })?,
        kind: kind.ok_or_else(|| {
            AppError::PlanningDocumentInvalid("document kind is missing".to_owned())
        })?,
        key: key.ok_or_else(|| {
            AppError::PlanningDocumentInvalid("document key is missing".to_owned())
        })?,
        status,
        repositories,
    };
    if front_matter.kind != DocumentKind::WorkItem && front_matter.status.is_some() {
        return Err(AppError::PlanningDocumentInvalid(
            "only Work-item documents have status".to_owned(),
        ));
    }
    let body = body.strip_prefix('\n').unwrap_or(body);
    if body.trim().is_empty() {
        return Err(AppError::PlanningDocumentInvalid(
            "document body is empty".to_owned(),
        ));
    }
    Ok((front_matter, normalise_body(body)))
}

fn document_kind(kind: DocumentKind) -> Result<&'static str, AppError> {
    match kind {
        DocumentKind::Epic => Ok("epic"),
        DocumentKind::Feature => Ok("feature"),
        DocumentKind::WorkItem => Ok("work_item"),
        DocumentKind::RepositoryInstructions => Err(AppError::PlanningDocumentInvalid(
            "repository instructions are not stored as hierarchy documents".to_owned(),
        )),
    }
}

fn parse_document_kind(value: &str) -> Result<DocumentKind, AppError> {
    match value {
        "epic" => Ok(DocumentKind::Epic),
        "feature" => Ok(DocumentKind::Feature),
        "work_item" => Ok(DocumentKind::WorkItem),
        _ => Err(AppError::PlanningDocumentInvalid(
            "document kind is unsupported".to_owned(),
        )),
    }
}

fn work_item_status(status: WorkItemStatus) -> &'static str {
    match status {
        WorkItemStatus::Backlog => "backlog",
        WorkItemStatus::Ready => "ready",
        WorkItemStatus::InProgress => "in_progress",
        WorkItemStatus::Blocked => "blocked",
        WorkItemStatus::Review => "review",
        WorkItemStatus::Done => "done",
        WorkItemStatus::Cancelled => "cancelled",
    }
}

fn parse_work_item_status(value: &str) -> Result<WorkItemStatus, AppError> {
    match value {
        "backlog" => Ok(WorkItemStatus::Backlog),
        "ready" => Ok(WorkItemStatus::Ready),
        "in_progress" => Ok(WorkItemStatus::InProgress),
        "blocked" => Ok(WorkItemStatus::Blocked),
        "review" => Ok(WorkItemStatus::Review),
        "done" => Ok(WorkItemStatus::Done),
        "cancelled" => Ok(WorkItemStatus::Cancelled),
        _ => Err(AppError::PlanningDocumentInvalid(
            "Work-item status is unsupported".to_owned(),
        )),
    }
}

fn normalise_body(body: &str) -> String {
    format!("{}\n", body.trim())
}

fn content_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn toml_string(value: &str) -> String {
    format!("{value:?}")
}

fn git_text<const N: usize>(cwd: &Path, arguments: [&str; N]) -> Result<String, AppError> {
    successful_git(
        Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(arguments)
            .output()
            .map_err(AppError::GitIo)?,
    )
}

fn successful_git(output: Output) -> Result<String, AppError> {
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(AppError::PlanningGit {
            message: if message.is_empty() {
                format!("Git exited with {}", output.status)
            } else {
                message
            },
        });
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim_end_matches(['\r', '\n']).to_owned())
        .map_err(|_| AppError::GitOutputEncoding)
}

fn planning_io(operation: &'static str, path: &Path, source: std::io::Error) -> AppError {
    AppError::PlanningStoreIo {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

fn copy_store(source: &Path, destination: &Path) -> Result<(), AppError> {
    for entry in fs::read_dir(source)
        .map_err(|error| planning_io("reading the export source", source, error))?
    {
        let entry =
            entry.map_err(|error| planning_io("reading the export source", source, error))?;
        if entry.file_name() == OsStr::new(".git") {
            continue;
        }
        let target = destination.join(entry.file_name());
        if entry
            .file_type()
            .map_err(|error| planning_io("reading an export entry", &entry.path(), error))?
            .is_dir()
        {
            fs::create_dir_all(&target)
                .map_err(|error| planning_io("creating an export directory", &target, error))?;
            copy_store(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)
                .map_err(|error| planning_io("copying an export file", &target, error))?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn paths_equal(left: &Path, right: &Path) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

#[cfg(not(windows))]
fn paths_equal(left: &Path, right: &Path) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    use tempfile::TempDir;
    use workboard_core::{DocumentId, DocumentKind, Slug, WorkItemStatus};

    use super::{DocumentFrontMatter, PlanningStore};
    use crate::AppError;

    fn configured_store(directory: &TempDir) -> PlanningStore {
        let path = directory.path().join("store");
        let store = PlanningStore::create_or_link(&path).expect("create store");
        for arguments in [
            ["config", "user.name", "Workboard Test"],
            ["config", "user.email", "workboard@example.invalid"],
        ] {
            assert!(
                Command::new("git")
                    .arg("-C")
                    .arg(store.root())
                    .args(arguments)
                    .status()
                    .expect("run Git config")
                    .success()
            );
        }
        store
    }

    #[test]
    fn creates_a_git_store_and_round_trips_canonical_front_matter() {
        let directory = TempDir::new().expect("temporary directory");
        let store = configured_store(&directory);
        let workspace = Slug::new("concertable").expect("workspace slug");
        let epic = Slug::new("launch").expect("epic slug");
        let config = store
            .initialise_workspace(&workspace, "Concertable")
            .expect("workspace config");
        store
            .commit_paths([config.as_path()], "Initialise Concertable workspace")
            .expect("commit workspace");
        let path = PlanningStore::epic_path(&workspace, &epic);
        let front_matter = DocumentFrontMatter {
            id: DocumentId::generate(),
            kind: DocumentKind::Epic,
            key: "launch".to_owned(),
            status: None,
            repositories: vec![Slug::new("concertable-code").expect("repository slug")],
        };
        let published = store
            .publish_new(
                &path,
                &front_matter,
                "# Launch\n\n## Outcome\n\nShip it.",
                "Create Launch epic",
            )
            .expect("publish epic");
        let read = store.read_document(&path).expect("read epic");

        assert_eq!(read.front_matter, front_matter);
        assert_eq!(read.body, "# Launch\n\n## Outcome\n\nShip it.\n");
        assert_eq!(read.content_hash, published.content_hash);
        assert_eq!(read.observed_commit, published.observed_commit);
    }

    #[test]
    fn round_trips_epic_feature_and_work_item_documents() {
        let directory = TempDir::new().expect("temporary directory");
        let store = configured_store(&directory);
        let workspace = Slug::new("concertable").expect("workspace slug");
        let epic = Slug::new("launch").expect("epic slug");
        let feature = Slug::new("availability").expect("feature slug");
        let work_item = Slug::new("api").expect("Work-item slug");
        let repository = Slug::new("concertable-code").expect("repository slug");
        let documents = [
            (
                PlanningStore::epic_path(&workspace, &epic),
                DocumentFrontMatter {
                    id: DocumentId::generate(),
                    kind: DocumentKind::Epic,
                    key: "launch".to_owned(),
                    status: None,
                    repositories: vec![repository.clone()],
                },
                "# Launch\n\n## Outcome\n\nShip.",
            ),
            (
                PlanningStore::feature_path(&workspace, &epic, &feature),
                DocumentFrontMatter {
                    id: DocumentId::generate(),
                    kind: DocumentKind::Feature,
                    key: "launch/availability".to_owned(),
                    status: None,
                    repositories: vec![repository.clone()],
                },
                "# Availability\n\n## Design\n\nDesign.",
            ),
            (
                PlanningStore::work_item_path(&workspace, &epic, &feature, &work_item),
                DocumentFrontMatter {
                    id: DocumentId::generate(),
                    kind: DocumentKind::WorkItem,
                    key: "launch/availability/api".to_owned(),
                    status: Some(WorkItemStatus::InProgress),
                    repositories: vec![repository],
                },
                "# Availability API\n\n## Next action\n\nImplement.",
            ),
        ];
        for (path, front_matter, body) in documents {
            store
                .publish_new(&path, &front_matter, body, "Publish hierarchy document")
                .expect("publish hierarchy document");
            let stored = store.read_document(&path).expect("read hierarchy document");
            assert_eq!(stored.front_matter, front_matter);
            assert_eq!(stored.body, format!("{}\n", body.trim()));
        }
    }

    #[test]
    fn rejects_traversal_and_detects_external_edits() {
        let directory = TempDir::new().expect("temporary directory");
        let store = configured_store(&directory);
        assert!(matches!(
            store.read_document(Path::new("../outside.md")),
            Err(AppError::PlanningDocumentInvalid(_))
        ));
        let path = Path::new("workspaces/demo/epics/launch/EPIC.md");
        let front_matter = DocumentFrontMatter {
            id: DocumentId::generate(),
            kind: DocumentKind::Epic,
            key: "launch".to_owned(),
            status: None,
            repositories: Vec::new(),
        };
        let published = store
            .publish_new(path, &front_matter, "# Launch", "Create epic")
            .expect("publish epic");
        fs::write(store.root().join(path), "external edit").expect("write external edit");
        assert!(matches!(
            store.publish_update(
                path,
                &published.content_hash,
                &front_matter,
                "# Updated launch",
                "Update epic"
            ),
            Err(AppError::PlanningDocumentConcurrentEdit(_))
        ));
        assert_eq!(
            fs::read_to_string(store.root().join(path)).expect("read external edit"),
            "external edit"
        );
    }

    #[test]
    fn failed_commit_is_reported_without_discarding_the_document() {
        let directory = TempDir::new().expect("temporary directory");
        let store = configured_store(&directory);
        let hook = store.root().join(".git/hooks/pre-commit");
        fs::write(&hook, "#!/bin/sh\nexit 1\n").expect("write rejecting hook");
        let path = Path::new("workspaces/demo/epics/launch/features/api/work-items/route.md");
        let front_matter = DocumentFrontMatter {
            id: DocumentId::generate(),
            kind: DocumentKind::WorkItem,
            key: "launch/api/route".to_owned(),
            status: Some(WorkItemStatus::Ready),
            repositories: Vec::new(),
        };
        assert!(matches!(
            store.publish_new(path, &front_matter, "# Route", "Create route Work item"),
            Err(AppError::PlanningGit { .. })
        ));
        assert!(store.root().join(path).is_file());
    }

    #[test]
    fn export_excludes_git_internals() {
        let directory = TempDir::new().expect("temporary directory");
        let store = configured_store(&directory);
        let workspace = Slug::new("demo").expect("workspace slug");
        store
            .initialise_workspace(&workspace, "Demo")
            .expect("workspace config");
        let export = directory.path().join("export");
        store.export(&export).expect("export store");
        assert!(export.join("workspaces/demo/workspace.toml").is_file());
        assert!(!export.join(".git").exists());
    }
}
