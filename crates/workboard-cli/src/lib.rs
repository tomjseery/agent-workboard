#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use directories::{ProjectDirs, UserDirs};
use serde::Serialize;
use workboard_application::AppError;
use workboard_application::legacy_import::preview_context_catalogue;
use workboard_application::workspace::{
    CreateEpic, InitialiseWorkspace, RegisterRepository, WorkboardApplication,
};
use workboard_core::{Feature, Slug, WorkItem, WorkspaceId};

use crate::selector::{SelectionCandidate, SelectionResult};

mod board;
mod selector;

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
    Feature(FeatureArgs),
    Work(WorkArgs),
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
struct FeatureArgs {
    #[command(subcommand)]
    command: FeatureCommand,
}

#[derive(Debug, Subcommand)]
enum FeatureCommand {
    Open { feature: Option<String> },
}

#[derive(Debug, Args)]
struct WorkArgs {
    #[command(subcommand)]
    command: WorkCommand,
}

#[derive(Debug, Subcommand)]
enum WorkCommand {
    Open { work_item: Option<String> },
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
    let cli = Cli::parse();
    if cli.command.is_none() && !cli.json && io::stdout().is_terminal() {
        if let Err(error) = run_interactive_board(&cli) {
            eprintln!("{}: {error}", error.code());
            std::process::exit(1);
        }
        return;
    }
    match execute(cli) {
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
            let human = board::plain(&snapshot);
            output(&snapshot, cli.json, human)
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
        Some(Command::Feature(FeatureArgs {
            command: FeatureCommand::Open { feature },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let feature = select_feature(&snapshot, feature.as_deref(), cli.json)?;
            output(
                feature,
                cli.json,
                format!("{} ({}) — {:?}", feature.title, feature.id, feature.state),
            )
        }
        Some(Command::Work(WorkArgs {
            command: WorkCommand::Open { work_item },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let work_item = select_work_item(&snapshot, work_item.as_deref(), cli.json)?;
            output(
                work_item,
                cli.json,
                format!(
                    "{} ({}) — {:?}",
                    work_item.title, work_item.key, work_item.status
                ),
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

fn run_interactive_board(cli: &Cli) -> Result<(), AppError> {
    let current_directory = std::env::current_dir().map_err(AppError::GitIo)?;
    let database = cli
        .database
        .as_ref()
        .map(|path| absolute(&current_directory, path))
        .map_or_else(default_database_path, Ok)?;
    let application = WorkboardApplication::open(database)?;
    let workspace_id = resolve_workspace(&application, cli.workspace)?;
    board::run(application.snapshot(workspace_id)?)
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

fn select_feature<'a>(
    snapshot: &'a workboard_core::WorkspaceSnapshot,
    query: Option<&str>,
    structured: bool,
) -> Result<&'a Feature, AppError> {
    let candidates = snapshot.features.iter().map(|feature| {
        let epic = snapshot
            .epics
            .iter()
            .find(|epic| epic.id == feature.epic_id)
            .map_or("", |epic| epic.title.as_str());
        SelectionCandidate {
            id: feature.id.to_string(),
            key: Some(format!(
                "{}/{}",
                snapshot
                    .epics
                    .iter()
                    .find(|epic| epic.id == feature.epic_id)
                    .map_or("", |epic| epic.slug.as_str()),
                feature.slug
            )),
            label: feature.title.clone(),
            metadata: format!("{epic} {:?}", feature.state),
        }
    });
    let candidate = select_candidate("Feature", query, candidates.collect(), structured)?;
    snapshot
        .features
        .iter()
        .find(|feature| feature.id.to_string() == candidate.id)
        .ok_or_else(|| AppError::Domain("selected Feature is unavailable".to_owned()))
}

fn select_work_item<'a>(
    snapshot: &'a workboard_core::WorkspaceSnapshot,
    query: Option<&str>,
    structured: bool,
) -> Result<&'a WorkItem, AppError> {
    let candidates = snapshot.work_items.iter().map(|item| SelectionCandidate {
        id: item.id.to_string(),
        key: Some(item.key.to_string()),
        label: item.title.clone(),
        metadata: format!("{:?}", item.status),
    });
    let candidate = select_candidate("Work item", query, candidates.collect(), structured)?;
    snapshot
        .work_items
        .iter()
        .find(|item| item.id.to_string() == candidate.id)
        .ok_or_else(|| AppError::Domain("selected Work item is unavailable".to_owned()))
}

fn select_candidate(
    kind: &str,
    query: Option<&str>,
    candidates: Vec<SelectionCandidate>,
    structured: bool,
) -> Result<SelectionCandidate, AppError> {
    match selector::resolve(query, candidates) {
        SelectionResult::Empty => Err(AppError::External {
            code: "selection_empty".to_owned(),
            message: format!("no {kind} matches the requested selection"),
        }),
        SelectionResult::Selected(candidate) => Ok(candidate),
        SelectionResult::Picker(candidates) if !structured && io::stdout().is_terminal() => {
            let candidates = candidates
                .into_iter()
                .map(|candidate| candidate.candidate)
                .collect();
            board::pick(&format!("Select {kind}"), candidates)?.ok_or_else(|| AppError::External {
                code: "selection_cancelled".to_owned(),
                message: format!("{kind} selection was cancelled"),
            })
        }
        SelectionResult::Picker(candidates) => Err(AppError::External {
            code: "selection_required".to_owned(),
            message: format!(
                "{kind} selection is ambiguous; candidates: {}",
                candidates
                    .iter()
                    .map(|candidate| candidate.candidate.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }),
    }
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

    use crate::selector::SelectionCandidate;

    use super::{execute_from, select_candidate, slugify};

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
    fn command_selection_uses_exact_unambiguous_and_picker_fallback_rules() {
        let candidates = vec![
            SelectionCandidate {
                id: "feature-one".to_owned(),
                key: Some("launch/availability-api".to_owned()),
                label: "Availability API".to_owned(),
                metadata: String::new(),
            },
            SelectionCandidate {
                id: "feature-two".to_owned(),
                key: Some("launch/availability-ui".to_owned()),
                label: "Availability UI".to_owned(),
                metadata: String::new(),
            },
        ];
        assert_eq!(
            select_candidate("Feature", Some("feature-one"), candidates.clone(), true)
                .expect("exact ID")
                .id,
            "feature-one"
        );
        assert_eq!(
            select_candidate(
                "Feature",
                Some("launch/availability-api"),
                candidates.clone(),
                true,
            )
            .expect("exact key")
            .id,
            "feature-one"
        );
        let error = select_candidate("Feature", Some("availability"), candidates, true)
            .expect_err("ambiguous selection should require a picker");
        assert_eq!(error.code(), "selection_required");
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
