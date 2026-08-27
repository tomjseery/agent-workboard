#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use directories::{ProjectDirs, UserDirs};
use serde::Serialize;
use workboard_application::AppError;
use workboard_application::caller::{CallerIdentityProvider, EnvironmentCallerIdentity};
use workboard_application::hooks::{HookIngestionMutation, MAX_HOOK_INPUT_BYTES};
use workboard_application::integration::{
    INTEGRATION_OWNER, IntegrationConfirmation, IntegrationOperation, IntegrationRequest,
    IntegrationResponse,
};
use workboard_application::legacy_import::preview_context_catalogue;
use workboard_application::native_launch::SystemLaunchExecutor;
use workboard_application::native_sources::RefreshNativeSources;
use workboard_application::session_launch::BeginManagedSessionLaunch;
use workboard_application::workspace::{
    CreateEpic, InitialiseWorkspace, RegisterRepository, WorkboardApplication,
};
use workboard_core::{
    Checkout, CheckoutAvailability, Feature, HierarchyOwner, ManagedLaunchMode, ManagedSessionRole,
    NativeSession, Slug, Tool, WorkItem, WorkspaceId,
};

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
    Session(SessionArgs),
    Integration(IntegrationArgs),
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
    Open {
        work_item: Option<String>,
    },
    Start {
        work_item: Option<String>,
        #[arg(long, value_enum, default_value = "codex")]
        tool: ToolArg,
        #[arg(long)]
        terminal: Option<PathBuf>,
        #[arg(long)]
        native: Option<PathBuf>,
        #[arg(long)]
        checkout: Option<String>,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
}

#[derive(Debug, Args)]
struct SessionArgs {
    #[command(subcommand)]
    command: SessionCommand,
}

#[derive(Debug, Subcommand)]
enum SessionCommand {
    Refresh {
        #[arg(long, value_enum)]
        tool: ToolArg,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Resume {
        session: Option<String>,
        #[arg(long, value_enum)]
        tool: Option<ToolArg>,
        #[arg(long)]
        terminal: Option<PathBuf>,
        #[arg(long)]
        native: Option<PathBuf>,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    Adopt {
        work_item: Option<String>,
        #[arg(long, value_enum)]
        tool: Option<ToolArg>,
    },
}

#[derive(Debug, Args)]
struct IntegrationArgs {
    #[command(subcommand)]
    command: IntegrationCommand,
}

#[derive(Debug, Subcommand)]
enum IntegrationCommand {
    Status {
        #[arg(long, value_enum)]
        tool: ToolArg,
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long)]
        executable: Option<PathBuf>,
    },
    Preview {
        #[arg(long, value_enum)]
        tool: ToolArg,
        #[arg(long, value_enum, default_value = "install")]
        operation: IntegrationMutationArg,
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long)]
        executable: Option<PathBuf>,
    },
    Install(IntegrationMutationArgs),
    Repair(IntegrationMutationArgs),
    Disable(IntegrationMutationArgs),
    Remove(IntegrationMutationArgs),
    IngestHook {
        #[arg(long, value_enum)]
        tool: ToolArg,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        quiet: bool,
    },
}

#[derive(Debug, Args)]
struct IntegrationMutationArgs {
    #[arg(long, value_enum)]
    tool: ToolArg,
    #[arg(long)]
    home: Option<PathBuf>,
    #[arg(long)]
    executable: Option<PathBuf>,
    #[arg(long)]
    confirm: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum IntegrationMutationArg {
    Install,
    Repair,
    Disable,
    Remove,
}

impl From<IntegrationMutationArg> for IntegrationOperation {
    fn from(value: IntegrationMutationArg) -> Self {
        match value {
            IntegrationMutationArg::Install => Self::Install,
            IntegrationMutationArg::Repair => Self::Repair,
            IntegrationMutationArg::Disable => Self::Disable,
            IntegrationMutationArg::Remove => Self::Remove,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ToolArg {
    Claude,
    Codex,
}

impl From<ToolArg> for Tool {
    fn from(value: ToolArg) -> Self {
        match value {
            ToolArg::Claude => Self::Claude,
            ToolArg::Codex => Self::Codex,
        }
    }
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
        Ok(output) if !output.is_empty() => println!("{output}"),
        Ok(_) => {}
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
        Some(Command::Work(WorkArgs {
            command:
                WorkCommand::Start {
                    work_item,
                    tool,
                    terminal,
                    native,
                    checkout,
                    idempotency_key,
                },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let work_item = select_work_item(&snapshot, work_item.as_deref(), cli.json)?.clone();
            let checkout = match checkout {
                Some(query) => {
                    let selected = select_checkout(&snapshot, &query, &work_item, cli.json)?;
                    application.override_work_item_checkout(
                        work_item.id,
                        selected.id,
                        time::OffsetDateTime::now_utc(),
                    )?
                }
                None => application.effective_work_item_checkout(work_item.id)?,
            };
            let tool = Tool::from(tool);
            let now = time::OffsetDateTime::now_utc();
            let request = BeginManagedSessionLaunch {
                owner: HierarchyOwner::WorkItem(work_item.id),
                role: ManagedSessionRole::WorkItemExecution,
                tool,
                mode: ManagedLaunchMode::New,
                checkout_id: checkout.checkout_id,
                working_directory: checkout.path,
                title: work_item.title,
                terminal_executable: terminal.unwrap_or_else(default_terminal_executable),
                native_executable: native.unwrap_or_else(|| default_native_executable(tool)),
                idempotency_key: idempotency_key.unwrap_or_else(new_idempotency_key),
                created_at: now,
                expires_at: now + time::Duration::minutes(2),
                resume_context: None,
            };
            let prepared = application.session_launch().begin(request)?;
            application
                .session_launch()
                .execute(&prepared, &SystemLaunchExecutor)?;
            let binding = await_binding(&mut application, prepared.intent_id)?;
            output(
                &binding,
                cli.json,
                format!(
                    "Launched and bound {} session for {}",
                    tool_title(tool),
                    checkout.title
                ),
            )
        }
        Some(Command::Session(SessionArgs {
            command: SessionCommand::Refresh { tool, home },
        })) => {
            let tool = Tool::from(tool);
            let root = home
                .map(|path| absolute(&current_directory, &path))
                .map_or_else(|| default_native_home(tool), Ok)?;
            let outcome = application.native_sources().refresh(RefreshNativeSources {
                tool,
                root,
                observed_at: time::OffsetDateTime::now_utc(),
            })?;
            output(
                &outcome,
                cli.json,
                format!(
                    "Refreshed {} native sources: {} conversations, {} failures",
                    tool_title(tool),
                    outcome.conversation_count,
                    outcome.failures.len()
                ),
            )
        }
        Some(Command::Session(SessionArgs {
            command:
                SessionCommand::Resume {
                    session,
                    tool,
                    terminal,
                    native,
                    idempotency_key,
                },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let requested_tool = tool.map(Tool::from);
            let session =
                select_session(&snapshot, session.as_deref(), requested_tool, cli.json)?.clone();
            let target = application.managed_session_target(session.id)?;
            let context = application.native_sources().resume_context(
                session.id,
                target.checkout.path.clone(),
                target.checkout.title.clone(),
            )?;
            let now = time::OffsetDateTime::now_utc();
            let request = BeginManagedSessionLaunch {
                owner: target.owner,
                role: target.role,
                tool: target.tool,
                mode: ManagedLaunchMode::Resume(target.native_id),
                checkout_id: target.checkout.checkout_id,
                working_directory: target.checkout.path,
                title: target.checkout.title,
                terminal_executable: terminal.unwrap_or_else(default_terminal_executable),
                native_executable: native.unwrap_or_else(|| default_native_executable(target.tool)),
                idempotency_key: idempotency_key.unwrap_or_else(new_idempotency_key),
                created_at: now,
                expires_at: now + time::Duration::minutes(2),
                resume_context: Some(context),
            };
            let prepared = application.session_launch().begin(request)?;
            application
                .session_launch()
                .execute(&prepared, &SystemLaunchExecutor)?;
            let binding = await_binding(&mut application, prepared.intent_id)?;
            output(
                &binding,
                cli.json,
                format!(
                    "Resumed and bound exact {} session",
                    tool_title(target.tool)
                ),
            )
        }
        Some(Command::Session(SessionArgs {
            command: SessionCommand::Adopt { work_item, tool },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let work_item = select_work_item(&snapshot, work_item.as_deref(), cli.json)?.clone();
            let checkout = application.effective_work_item_checkout(work_item.id)?;
            let caller = EnvironmentCallerIdentity.identify(tool.map(Tool::from))?;
            let binding = application.session_launch().adopt_observed(
                HierarchyOwner::WorkItem(work_item.id),
                checkout.checkout_id,
                &caller.conversation,
                &current_directory,
                time::OffsetDateTime::now_utc(),
            )?;
            output(
                &binding,
                cli.json,
                format!("Adopted session into {}", work_item.title),
            )
        }
        Some(Command::Integration(IntegrationArgs {
            command: IntegrationCommand::IngestHook { tool, owner, quiet },
        })) => {
            if owner != INTEGRATION_OWNER {
                return Err(AppError::CallerIdentityMismatch);
            }
            let payload_json = read_hook_input()?;
            let outcome = application
                .session_launch()
                .ingest_hook(&HookIngestionMutation {
                    tool: tool.into(),
                    payload_json,
                    observed_at: time::OffsetDateTime::now_utc()
                        .format(&time::format_description::well_known::Rfc3339)
                        .map_err(|error| AppError::Domain(error.to_string()))?,
                    launch_token: std::env::var("WORKBOARD_LAUNCH_TOKEN")
                        .ok()
                        .filter(|value| !value.trim().is_empty()),
                    process: None,
                })?;
            if quiet {
                Ok(String::new())
            } else {
                output(&outcome, cli.json, "Native hook recorded".to_owned())
            }
        }
        Some(Command::Integration(IntegrationArgs {
            command:
                IntegrationCommand::Status {
                    tool,
                    home,
                    executable,
                },
        })) => execute_integration(
            &mut application,
            &current_directory,
            tool.into(),
            home,
            executable,
            IntegrationOperation::Status,
            None,
            cli.json,
        ),
        Some(Command::Integration(IntegrationArgs {
            command:
                IntegrationCommand::Preview {
                    tool,
                    operation,
                    home,
                    executable,
                },
        })) => execute_integration(
            &mut application,
            &current_directory,
            tool.into(),
            home,
            executable,
            IntegrationOperation::Preview,
            Some((operation.into(), None)),
            cli.json,
        ),
        Some(Command::Integration(IntegrationArgs { command })) => {
            let (operation, arguments) = match command {
                IntegrationCommand::Install(arguments) => {
                    (IntegrationOperation::Install, arguments)
                }
                IntegrationCommand::Repair(arguments) => (IntegrationOperation::Repair, arguments),
                IntegrationCommand::Disable(arguments) => {
                    (IntegrationOperation::Disable, arguments)
                }
                IntegrationCommand::Remove(arguments) => (IntegrationOperation::Remove, arguments),
                _ => unreachable!(),
            };
            execute_integration(
                &mut application,
                &current_directory,
                arguments.tool.into(),
                arguments.home,
                arguments.executable,
                operation,
                Some((operation, Some(arguments.confirm))),
                cli.json,
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

fn select_session<'a>(
    snapshot: &'a workboard_core::WorkspaceSnapshot,
    query: Option<&str>,
    tool: Option<Tool>,
    structured: bool,
) -> Result<&'a NativeSession, AppError> {
    let candidates = snapshot
        .sessions
        .iter()
        .filter(|session| tool.is_none_or(|tool| session.native.tool() == tool))
        .map(|session| SelectionCandidate {
            id: session.id.to_string(),
            key: Some(session.native.native_id().to_owned()),
            label: session.native.native_id().to_owned(),
            metadata: tool_title(session.native.tool()).to_owned(),
        })
        .collect();
    let candidate = select_candidate("session", query, candidates, structured)?;
    snapshot
        .sessions
        .iter()
        .find(|session| session.id.to_string() == candidate.id)
        .ok_or(AppError::ConversationNotFound)
}

fn select_checkout<'a>(
    snapshot: &'a workboard_core::WorkspaceSnapshot,
    query: &str,
    work_item: &WorkItem,
    structured: bool,
) -> Result<&'a Checkout, AppError> {
    let candidates = snapshot
        .checkouts
        .iter()
        .filter(|checkout| {
            checkout.availability == CheckoutAvailability::Available
                && work_item.repository_ids.contains(&checkout.repository_id)
        })
        .map(|checkout| {
            let path = checkout
                .paths
                .iter()
                .find(|path| path.observed_until.is_none())
                .map(|path| path.path.to_string_lossy().into_owned())
                .unwrap_or_default();
            SelectionCandidate {
                id: checkout.id.to_string(),
                key: checkout.branch.clone(),
                label: path,
                metadata: checkout.head.clone().unwrap_or_default(),
            }
        })
        .collect();
    let candidate = select_candidate("checkout", Some(query), candidates, structured)?;
    snapshot
        .checkouts
        .iter()
        .find(|checkout| checkout.id.to_string() == candidate.id)
        .ok_or(AppError::ResumeCheckoutRequired)
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

fn default_native_home(tool: Tool) -> Result<PathBuf, AppError> {
    let home = UserDirs::new()
        .map(|directories| directories.home_dir().to_path_buf())
        .ok_or(AppError::DataDirectoryUnavailable)?;
    Ok(match tool {
        Tool::Claude => home.join(".claude").join("projects"),
        Tool::Codex => home.join(".codex").join("sessions"),
    })
}

fn default_integration_home(tool: Tool) -> Result<PathBuf, AppError> {
    let home = UserDirs::new()
        .map(|directories| directories.home_dir().to_path_buf())
        .ok_or(AppError::DataDirectoryUnavailable)?;
    Ok(match tool {
        Tool::Claude => home.join(".claude"),
        Tool::Codex => home.join(".codex"),
    })
}

#[allow(clippy::too_many_arguments)]
fn execute_integration(
    application: &mut WorkboardApplication,
    current_directory: &Path,
    tool: Tool,
    home: Option<PathBuf>,
    executable: Option<PathBuf>,
    operation: IntegrationOperation,
    mutation: Option<(IntegrationOperation, Option<String>)>,
    json: bool,
) -> Result<String, AppError> {
    let native_home = home
        .map(|path| absolute(current_directory, &path))
        .map_or_else(|| default_integration_home(tool), Ok)?;
    let workboard_executable = executable
        .map(|path| absolute(current_directory, &path))
        .map_or_else(|| std::env::current_exe().map_err(AppError::GitIo), Ok)?;
    let (preview_operation, confirmation) = mutation.map_or((None, None), |(operation, token)| {
        (
            Some(operation),
            token.map(|token| IntegrationConfirmation { token }),
        )
    });
    let response = application.integrations().execute(
        IntegrationRequest {
            tool,
            native_home,
            workboard_executable,
            operation,
            preview_operation,
            confirmation,
        },
        time::OffsetDateTime::now_utc(),
    )?;
    let human = match &response {
        IntegrationResponse::Status { status } => {
            format!("{} integration: {:?}", tool_title(tool), status.state)
        }
        IntegrationResponse::Preview { preview } => format!(
            "{} integration preview: change={}, confirmation={}",
            tool_title(tool),
            preview.will_change,
            preview.confirmation_token
        ),
        IntegrationResponse::Mutation { outcome } => format!(
            "{} integration {:?}: changed={}, state={:?}",
            tool_title(tool),
            outcome.operation,
            outcome.changed,
            outcome.status.state
        ),
    };
    output(&response, json, human)
}

#[cfg(windows)]
fn default_terminal_executable() -> PathBuf {
    PathBuf::from("wt.exe")
}

#[cfg(target_os = "linux")]
fn default_terminal_executable() -> PathBuf {
    PathBuf::from("xdg-terminal-exec")
}

#[cfg(not(any(windows, target_os = "linux")))]
fn default_terminal_executable() -> PathBuf {
    PathBuf::new()
}

fn default_native_executable(tool: Tool) -> PathBuf {
    match tool {
        Tool::Claude => PathBuf::from("claude"),
        Tool::Codex => PathBuf::from("codex"),
    }
}

fn new_idempotency_key() -> String {
    workboard_core::LaunchIntentId::generate().to_string()
}

fn tool_title(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "Claude",
        Tool::Codex => "Codex",
    }
}

fn read_hook_input() -> Result<String, AppError> {
    let mut bytes = Vec::new();
    io::stdin()
        .take((MAX_HOOK_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(AppError::HookInputIo)?;
    if bytes.len() > MAX_HOOK_INPUT_BYTES {
        return Err(AppError::HookInputTooLarge {
            limit: MAX_HOOK_INPUT_BYTES,
        });
    }
    String::from_utf8(bytes).map_err(|error| AppError::InvalidHookInput(error.to_string()))
}

fn await_binding(
    application: &mut WorkboardApplication,
    intent_id: workboard_core::LaunchIntentId,
) -> Result<workboard_application::session_launch::ConfirmedSessionBinding, AppError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if let Some(binding) = application.session_launch().binding_for_intent(intent_id)? {
            return Ok(binding);
        }
        if std::time::Instant::now() >= deadline {
            return Err(AppError::External {
                code: "launch_binding_pending".to_owned(),
                message: format!(
                    "native process launched but no exact hook binding arrived for intent {intent_id}"
                ),
            });
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
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

    #[test]
    fn integration_preview_and_install_round_trip_through_the_cli() {
        let directory = TempDir::new().expect("temporary directory");
        let database = directory.path().join("workboard.sqlite");
        let native_home = directory.path().join(".claude");
        let executable = directory.path().join("workboard.exe");
        std::fs::create_dir(&native_home).expect("native home");
        std::fs::write(&executable, []).expect("workboard executable");
        let common = [
            "workboard",
            "--database",
            database.to_str().expect("database path"),
            "--json",
            "integration",
        ];
        let preview = execute_from(common.into_iter().chain([
            "preview",
            "--tool",
            "claude",
            "--home",
            native_home.to_str().expect("native home path"),
            "--executable",
            executable.to_str().expect("executable path"),
        ]))
        .expect("preview integration");
        let preview: serde_json::Value =
            serde_json::from_str(&preview).expect("parse preview output");
        let token = preview["preview"]["confirmationToken"]
            .as_str()
            .expect("confirmation token");
        let installed = execute_from(common.into_iter().chain([
            "install",
            "--tool",
            "claude",
            "--home",
            native_home.to_str().expect("native home path"),
            "--executable",
            executable.to_str().expect("executable path"),
            "--confirm",
            token,
        ]))
        .expect("install integration");
        let installed: serde_json::Value =
            serde_json::from_str(&installed).expect("parse install output");
        assert_eq!(installed["outcome"]["status"]["state"], "installed");
        assert!(native_home.join("settings.json").is_file());
    }
}
