#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use directories::{ProjectDirs, UserDirs};
use serde::Serialize;
use workboard_application::AppError;
use workboard_application::legacy_import::preview_context_catalogue;
use workboard_application::workspace::{
    CreateEpic, InitialiseWorkspace, RegisterRepository, WorkboardApplication,
};
use workboard_core::{Slug, WorkspaceId};

#[derive(Debug, Parser)]
#[command(
    name = "workboard",
    version,
    about = "Native agent work and planning board"
)]
struct Cli {
    #[arg(long, global = true, env = "WORKBOARD_DATABASE")]
    database: Option<PathBuf>,
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    workspace: Option<WorkspaceId>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init(InitArgs),
    Repository(RepositoryArgs),
    Epic(EpicArgs),
    Snapshot,
    Backup(DestinationArgs),
    Export(DestinationArgs),
    Import(ImportArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    #[arg(long)]
    store: Option<PathBuf>,
    #[arg(long)]
    slug: Option<String>,
    #[arg(long)]
    title: Option<String>,
}

#[derive(Debug, Args)]
struct RepositoryArgs {
    #[command(subcommand)]
    command: RepositoryCommand,
}

#[derive(Debug, Subcommand)]
enum RepositoryCommand {
    Add {
        path: Option<PathBuf>,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        title: Option<String>,
    },
}

#[derive(Debug, Args)]
struct EpicArgs {
    #[command(subcommand)]
    command: EpicCommand,
}

#[derive(Debug, Subcommand)]
enum EpicCommand {
    Create {
        title: String,
        #[arg(long)]
        slug: Option<String>,
    },
    Import {
        markdown: PathBuf,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        slug: Option<String>,
    },
}

#[derive(Debug, Args)]
struct DestinationArgs {
    destination: PathBuf,
}

#[derive(Debug, Args)]
struct ImportArgs {
    #[command(subcommand)]
    command: ImportCommand,
}

#[derive(Debug, Subcommand)]
enum ImportCommand {
    ContextCatalogue { database: PathBuf },
}

pub fn run() {
    match execute(Cli::parse()) {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("{}: {error}", error.code());
            std::process::exit(1);
        }
    }
}

pub fn execute_from<I, T>(arguments: I) -> Result<String, AppError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli =
        Cli::try_parse_from(arguments).map_err(|error| AppError::Domain(error.to_string()))?;
    execute(cli)
}

fn execute(cli: Cli) -> Result<String, AppError> {
    let current_directory = std::env::current_dir().map_err(AppError::GitIo)?;
    if let Some(Command::Import(ImportArgs {
        command: ImportCommand::ContextCatalogue { database },
    })) = cli.command.as_ref()
    {
        let preview = preview_context_catalogue(&absolute(&current_directory, database))?;
        return output(
            &preview,
            cli.json,
            format!(
                "Preview: {} repositories, {} sessions, {} associations, {} checkouts",
                preview.repositories,
                preview.native_sessions,
                preview.association_events,
                preview.checkouts
            ),
        );
    }
    let database = cli
        .database
        .map(|path| absolute(&current_directory, &path))
        .map_or_else(default_database_path, Ok)?;
    let mut application = WorkboardApplication::open(database)?;
    match cli.command {
        None => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            output(
                &snapshot,
                cli.json,
                format!(
                    "Agent Workboard: {} ({} Epics, {} Features, {} Work items)",
                    snapshot.workspace.title,
                    snapshot.epics.len(),
                    snapshot.features.len(),
                    snapshot.work_items.len()
                ),
            )
        }
        Some(Command::Init(arguments)) => {
            let default_title = current_name(&current_directory)?;
            let title = arguments.title.unwrap_or(default_title);
            let slug = parse_or_derive_slug(arguments.slug.as_deref(), &title)?;
            let store = arguments
                .store
                .map(|path| absolute(&current_directory, &path))
                .map_or_else(default_store_path, Ok)?;
            let snapshot = application.initialise_workspace(InitialiseWorkspace {
                slug,
                title,
                planning_store_path: store,
            })?;
            output(
                &snapshot,
                cli.json,
                format!(
                    "Initialised {} ({})",
                    snapshot.workspace.title, snapshot.workspace.id
                ),
            )
        }
        Some(Command::Repository(RepositoryArgs {
            command: RepositoryCommand::Add { path, slug, title },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let path = path
                .map(|path| absolute(&current_directory, &path))
                .unwrap_or(current_directory);
            let default_title = current_name(&path)?;
            let title = title.unwrap_or(default_title);
            let repository = application.register_repository(RegisterRepository {
                workspace_id,
                slug: parse_or_derive_slug(slug.as_deref(), &title)?,
                title,
                path,
            })?;
            output(
                &repository,
                cli.json,
                format!("Registered {} ({})", repository.title, repository.id),
            )
        }
        Some(Command::Epic(EpicArgs { command })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let (title, slug, body) = match command {
                EpicCommand::Create { title, slug } => {
                    let slug = parse_or_derive_slug(slug.as_deref(), &title)?;
                    (title, slug, String::new())
                }
                EpicCommand::Import {
                    markdown,
                    title,
                    slug,
                } => {
                    let path = absolute(&current_directory, &markdown);
                    let body =
                        fs::read_to_string(&path).map_err(|source| AppError::PlanningStoreIo {
                            operation: "reading the imported Epic",
                            path: path.clone(),
                            source,
                        })?;
                    let title = title.or_else(|| markdown_title(&body)).ok_or_else(|| {
                        AppError::PlanningDocumentInvalid(
                            "the imported Epic needs an H1 title or --title".to_owned(),
                        )
                    })?;
                    let slug = parse_or_derive_slug(slug.as_deref(), &title)?;
                    (title, slug, body)
                }
            };
            let epic = application.create_epic(CreateEpic {
                workspace_id,
                slug,
                title,
                body,
            })?;
            output(
                &epic,
                cli.json,
                format!("Created Epic {} ({})", epic.title, epic.id),
            )
        }
        Some(Command::Snapshot) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            output(&snapshot, cli.json, snapshot.workspace.title.clone())
        }
        Some(Command::Backup(arguments)) => {
            let destination = absolute(&current_directory, &arguments.destination);
            let health = application.backup_database(&destination)?;
            output(
                &health,
                cli.json,
                format!("Verified database backup at {}", destination.display()),
            )
        }
        Some(Command::Export(arguments)) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let destination = absolute(&current_directory, &arguments.destination);
            application.export_planning_store(workspace_id, &destination)?;
            if cli.json {
                serde_json::to_string_pretty(&serde_json::json!({
                    "destination": destination,
                    "exported": true
                }))
                .map_err(Into::into)
            } else {
                Ok(format!(
                    "Exported planning store to {}",
                    destination.display()
                ))
            }
        }
        Some(Command::Import(_)) => unreachable!(),
    }
}

fn output<T: Serialize>(value: &T, json: bool, human: String) -> Result<String, AppError> {
    if json {
        serde_json::to_string_pretty(value).map_err(Into::into)
    } else {
        Ok(human)
    }
}

fn resolve_workspace(
    application: &WorkboardApplication,
    requested: Option<WorkspaceId>,
) -> Result<WorkspaceId, AppError> {
    requested.map_or_else(|| application.sole_workspace_id(), Ok)
}

fn default_database_path() -> Result<PathBuf, AppError> {
    ProjectDirs::from("dev", "Agent Workboard", "Agent Workboard")
        .map(|directories| directories.data_local_dir().join("workboard.sqlite"))
        .ok_or(AppError::DataDirectoryUnavailable)
}

fn default_store_path() -> Result<PathBuf, AppError> {
    UserDirs::new()
        .map(|directories| directories.home_dir().join("agent-workboard-store"))
        .ok_or(AppError::DataDirectoryUnavailable)
}

fn absolute(current_directory: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_directory.join(path)
    }
}

fn current_name(path: &Path) -> Result<String, AppError> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(str::to_owned)
        .ok_or_else(|| AppError::Domain(format!("path has no usable name: {}", path.display())))
}

fn parse_or_derive_slug(value: Option<&str>, title: &str) -> Result<Slug, AppError> {
    let value = value.map_or_else(|| slugify(title), str::to_owned);
    Slug::new(value).map_err(|error| AppError::Domain(error.to_string()))
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character);
            separator = false;
        } else {
            separator = true;
        }
    }
    slug
}

fn markdown_title(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use tempfile::TempDir;

    use super::{execute_from, slugify};

    #[test]
    fn derives_safe_slugs() {
        assert_eq!(slugify("Venue Availability API"), "venue-availability-api");
        assert_eq!(slugify("  Mixed___punctuation  "), "mixed-punctuation");
    }

    #[test]
    fn invalid_command_returns_a_typed_error() {
        let error =
            execute_from(["workboard", "epic", "create"]).expect_err("missing title should fail");
        assert_eq!(error.code(), "domain");
    }

    #[test]
    fn init_and_epic_create_support_structured_output() {
        let directory = TempDir::new().expect("temporary directory");
        let database = directory.path().join("workboard.sqlite");
        let planning = directory.path().join("planning");
        let store = workboard_application::planning_store::PlanningStore::create_or_link(&planning)
            .expect("create planning store");
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
                    .expect("configure Git")
                    .success()
            );
        }
        let init = execute_from([
            "workboard",
            "--database",
            database.to_str().expect("database path"),
            "--json",
            "init",
            "--store",
            planning.to_str().expect("planning path"),
            "--slug",
            "demo",
            "--title",
            "Demo",
        ])
        .expect("initialise through CLI");
        let init: serde_json::Value = serde_json::from_str(&init).expect("parse init output");
        assert_eq!(init["workspace"]["title"], "Demo");

        let epic = execute_from([
            "workboard",
            "--database",
            database.to_str().expect("database path"),
            "--json",
            "epic",
            "create",
            "Launch",
        ])
        .expect("create Epic through CLI");
        let epic: serde_json::Value = serde_json::from_str(&epic).expect("parse Epic output");
        assert_eq!(epic["title"], "Launch");
        assert!(
            planning
                .join("workspaces/demo/epics/launch/EPIC.md")
                .is_file()
        );
    }
}
