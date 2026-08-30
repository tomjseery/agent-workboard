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
use workboard_application::checkout::{
    AdoptFeatureCheckout, PrepareFeatureCheckout, PrepareWorkItemCheckout,
};
use workboard_application::concertable_import::{
    ConcertableImportPreview, preview_concertable_plans,
};
use workboard_application::hooks::{HookIngestionMutation, MAX_HOOK_INPUT_BYTES};
use workboard_application::integration::{
    INTEGRATION_OWNER, IntegrationConfirmation, IntegrationOperation, IntegrationRequest,
    IntegrationResponse, IntegrationState,
};
use workboard_application::legacy_import::{
    ImportedSessionCandidate, LegacyImportPreview, snapshot_context_catalogue,
};
use workboard_application::native_launch::{SystemLaunchExecutor, SystemProcessTerminator};
use workboard_application::native_sources::RefreshNativeSources;
use workboard_application::planning_workflow::FeatureProposal;
use workboard_application::planning_workflow::{CreateFeaturePlanning, planner_bootstrap_prompt};
use workboard_application::session_launch::BeginManagedSessionLaunch;
use workboard_application::workflow_operations::{
    CheckpointWorkItem, RequestManagedSession, work_item_bootstrap_prompt,
};
use workboard_application::workspace::{
    CreateEpic, InitialiseWorkspace, RegisterRepository, WorkboardApplication,
};
use workboard_core::{
    Checkout, CheckoutAvailability, Epic, Feature, HierarchyOwner, ManagedLaunchMode,
    ManagedSessionRole, NativeSession, NextActionKind, Repository, Slug, Tool, WorkItem,
    WorkItemId, WorkspaceId,
};

use crate::selector::{SelectionCandidate, SelectionResult};

mod board;
mod mcp;
mod recovery;
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
    Workflow(WorkflowArgs),
    Recover(RecoverArgs),
    Mcp,
    #[command(alias = "snapshot")]
    Show,
    Backup(DestinationArgs),
    Export(DestinationArgs),
    Import(ImportArgs),
}

#[derive(Debug, Args)]
struct RecoverArgs {
    #[arg(long)]
    since: Option<String>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    yes: bool,
    #[arg(long)]
    replace_unresumable: bool,
    #[arg(long = "session")]
    sessions: Vec<String>,
    #[arg(long)]
    terminal: Option<PathBuf>,
    #[arg(long)]
    claude: Option<PathBuf>,
    #[arg(long)]
    codex: Option<PathBuf>,
    #[arg(long)]
    idempotency_key: Option<String>,
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
    Continue(Box<EpicContinueArgs>),
}

#[derive(Debug, Args)]
struct EpicContinueArgs {
    epic: Option<String>,
    #[arg(long)]
    repository: Option<String>,
    #[arg(long, value_enum, default_value = "codex")]
    tool: ToolArg,
    #[arg(long)]
    terminal: Option<PathBuf>,
    #[arg(long)]
    native: Option<PathBuf>,
    #[arg(long)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Args)]
struct FeatureArgs {
    #[command(subcommand)]
    command: FeatureCommand,
}

#[derive(Debug, Subcommand)]
enum FeatureCommand {
    Create(Box<FeatureCreateArgs>),
    Open {
        feature: Option<String>,
    },
    UseCheckout {
        feature: Option<String>,
        #[arg(long)]
        checkout: String,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    Approve {
        feature: Option<String>,
    },
    Reject {
        feature: Option<String>,
    },
    Publish {
        feature: Option<String>,
    },
}

#[derive(Debug, Args)]
struct FeatureCreateArgs {
    title: String,
    #[arg(long)]
    slug: Option<String>,
    #[arg(long)]
    epic: Option<String>,
    #[arg(long)]
    repository: Option<String>,
    #[arg(long)]
    worktree: Option<PathBuf>,
    #[arg(long)]
    branch: Option<String>,
    #[arg(long)]
    base: Option<String>,
    #[arg(long, value_enum, default_value = "codex")]
    tool: ToolArg,
    #[arg(long)]
    terminal: Option<PathBuf>,
    #[arg(long)]
    native: Option<PathBuf>,
    #[arg(long)]
    idempotency_key: Option<String>,
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
        #[arg(long)]
        repository: Option<String>,
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
    RemoveFromRestore {
        session: Option<String>,
        #[arg(long, default_value = "removed by user")]
        reason: String,
    },
    Close {
        session: Option<String>,
        #[arg(long, default_value = "closed by user")]
        reason: String,
    },
    ImportedCandidates {
        query: Option<String>,
    },
    AdoptImported {
        session: String,
        work_item: Option<String>,
    },
    IgnoreImported {
        session: String,
    },
}

#[derive(Debug, Args)]
struct IntegrationArgs {
    #[command(subcommand)]
    command: IntegrationCommand,
}

#[derive(Debug, Args)]
struct WorkflowArgs {
    #[command(subcommand)]
    command: WorkflowCommand,
}

#[derive(Debug, Subcommand)]
enum WorkflowCommand {
    ReadHierarchy(RequestFileArgs),
    SubmitFeatureProposal(RequestFileArgs),
    PublishFeature(RequestFileArgs),
    CheckpointWorkItem(RequestFileArgs),
    RequestSession(RequestFileArgs),
}

#[derive(Debug, Args)]
struct RequestFileArgs {
    #[arg(long)]
    request: PathBuf,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeatureProposalRequest {
    feature_id: workboard_core::FeatureId,
    idempotency_key: String,
    proposal: FeatureProposal,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeaturePublicationRequest {
    feature_id: workboard_core::FeatureId,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkItemCheckpointRequest {
    work_item_id: WorkItemId,
    next_action: NextActionKind,
    summary: String,
    idempotency_key: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedSessionRequest {
    work_item_id: WorkItemId,
    repository_id: Option<workboard_core::RepositoryId>,
    tool: Tool,
    idempotency_key: String,
    terminal: Option<PathBuf>,
    native: Option<PathBuf>,
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
    ContextCatalogue {
        #[command(subcommand)]
        command: ContextCatalogueCommand,
    },
    ConcertablePlans {
        #[command(subcommand)]
        command: ConcertablePlansCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ContextCatalogueCommand {
    Preview {
        database: PathBuf,
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Apply {
        preview: PathBuf,
        #[arg(long)]
        repository: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ConcertablePlansCommand {
    Preview {
        repository: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Apply {
        preview: PathBuf,
        #[arg(long)]
        repository: Option<String>,
    },
}

pub fn run() {
    let cli = Cli::parse();
    if matches!(cli.command, Some(Command::Mcp)) {
        let result = (|| {
            let current_directory = std::env::current_dir().map_err(AppError::GitIo)?;
            let database = cli
                .database
                .as_ref()
                .map(|path| absolute(&current_directory, path))
                .map_or_else(default_database_path, Ok)?;
            mcp::run(database)
        })();
        if let Err(error) = result {
            eprintln!("{}: {error}", error.code());
            std::process::exit(1);
        }
        return;
    }
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
        command:
            ImportCommand::ContextCatalogue {
                command:
                    ContextCatalogueCommand::Preview {
                        database,
                        backup,
                        output: destination,
                    },
            },
    })) = cli.command.as_ref()
    {
        let database = absolute(&current_directory, database);
        let backup = absolute(&current_directory, backup);
        let destination = absolute(&current_directory, destination);
        if destination.exists() {
            return Err(AppError::Domain(format!(
                "import preview already exists: {}",
                destination.display()
            )));
        }
        let preview = snapshot_context_catalogue(&database, &backup)?;
        let bytes = serde_json::to_vec_pretty(&preview)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|source| AppError::StorageIo {
                operation: "creating the legacy import preview directory",
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(&destination, bytes).map_err(|source| AppError::StorageIo {
            operation: "writing the legacy import preview",
            path: destination.clone(),
            source,
        })?;
        return output(
            &preview,
            cli.json,
            format!(
                "Backed up and previewed {} repositories, {} sessions, {} associations, and {} checkouts at {}",
                preview.repositories,
                preview.native_sessions,
                preview.association_events,
                preview.checkouts,
                destination.display()
            ),
        );
    }
    if let Some(Command::Import(ImportArgs {
        command:
            ImportCommand::ConcertablePlans {
                command:
                    ConcertablePlansCommand::Preview {
                        repository,
                        output: destination,
                    },
            },
    })) = cli.command.as_ref()
    {
        let repository = absolute(&current_directory, repository);
        let destination = absolute(&current_directory, destination);
        if destination.exists() {
            return Err(AppError::Domain(format!(
                "import preview already exists: {}",
                destination.display()
            )));
        }
        let preview = preview_concertable_plans(&repository)?;
        let bytes = serde_json::to_vec_pretty(&preview)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|source| AppError::PlanningStoreIo {
                operation: "creating the import preview directory",
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(&destination, bytes).map_err(|source| AppError::PlanningStoreIo {
            operation: "writing the import preview",
            path: destination.clone(),
            source,
        })?;
        return output(
            &preview,
            cli.json,
            format!(
                "Wrote editable Concertable import preview to {}",
                destination.display()
            ),
        );
    }
    let database = cli
        .database
        .map(|path| absolute(&current_directory, &path))
        .map_or_else(default_database_path, Ok)?;
    if cli.command.is_none() {
        let snapshot = client_snapshot(&database, cli.workspace)?;
        let human = board::plain(&snapshot);
        return output(&snapshot, cli.json, human);
    }
    if matches!(cli.command.as_ref(), Some(Command::Show)) {
        let snapshot = client_snapshot(&database, cli.workspace)?;
        let title = snapshot.workspace.title.clone();
        return output(&snapshot, cli.json, title);
    }
    let mut application = WorkboardApplication::open(database)?;
    match cli.command {
        None => unreachable!(),
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
        Some(Command::Epic(EpicArgs {
            command: EpicCommand::Continue(arguments),
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            execute_epic_continue(&mut application, workspace_id, *arguments, cli.json)
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
                EpicCommand::Continue(_) => unreachable!(),
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
            command: FeatureCommand::Create(arguments),
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            execute_feature_create(
                &mut application,
                &current_directory,
                workspace_id,
                *arguments,
                cli.json,
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
        Some(Command::Feature(FeatureArgs {
            command:
                FeatureCommand::UseCheckout {
                    feature,
                    checkout,
                    idempotency_key,
                },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let feature = select_feature(&snapshot, feature.as_deref(), cli.json)?;
            let checkout = select_available_checkout(&snapshot, &checkout, cli.json)?;
            let outcome =
                application
                    .checkout_service()
                    .adopt_feature_checkout(AdoptFeatureCheckout {
                        feature_id: feature.id,
                        checkout_id: checkout.id,
                        idempotency_key: idempotency_key.unwrap_or_else(new_idempotency_key),
                        observed_at: time::OffsetDateTime::now_utc(),
                    })?;
            output(
                &outcome,
                cli.json,
                format!("Assigned {} to {}", outcome.path.display(), feature.title),
            )
        }
        Some(Command::Feature(FeatureArgs {
            command: FeatureCommand::Reject { feature },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let feature = select_feature(&snapshot, feature.as_deref(), cli.json)?.clone();
            let outcome = application
                .planning_workflows()
                .reject_proposal(feature.id, time::OffsetDateTime::now_utc())?;
            output(
                &outcome,
                cli.json,
                format!("Rejected the proposal for {}", feature.title),
            )
        }
        Some(Command::Feature(FeatureArgs {
            command: FeatureCommand::Approve { feature },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let feature = select_feature(&snapshot, feature.as_deref(), cli.json)?.clone();
            let now = time::OffsetDateTime::now_utc();
            application
                .planning_workflows()
                .approve_proposal(feature.id, now)?;
            let outcome = application
                .planning_workflows()
                .publish_approved(feature.id, now)?;
            let next = outcome.first_work_item_id.map_or_else(
                || "No first Work item was selected.".to_owned(),
                |id| format!("Start it with: workboard work start {id}"),
            );
            output(
                &outcome,
                cli.json,
                format!(
                    "Published {} in commit {}. {next}",
                    feature.title, outcome.commit
                ),
            )
        }
        Some(Command::Feature(FeatureArgs {
            command: FeatureCommand::Publish { feature },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let feature = select_feature(&snapshot, feature.as_deref(), cli.json)?.clone();
            let outcome = application
                .planning_workflows()
                .publish_approved(feature.id, time::OffsetDateTime::now_utc())?;
            output(
                &outcome,
                cli.json,
                format!("Published {} in commit {}", feature.title, outcome.commit),
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
                    repository,
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
            let tool = Tool::from(tool);
            let now = time::OffsetDateTime::now_utc();
            let launch_idempotency_key = idempotency_key.unwrap_or_else(new_idempotency_key);
            if checkout.is_some() {
                return Err(AppError::CheckoutReconciliation {
                    code: "explicit_write_checkout_unsupported".to_owned(),
                    message: "write-capable Work-item launches use a derived isolated checkout"
                        .to_owned(),
                });
            }
            let repository_id = if let Some(query) = repository {
                let repository = select_repository(&snapshot, Some(&query), cli.json)?;
                if !work_item.repository_ids.contains(&repository.id) {
                    return Err(AppError::WorkItemRepositoryMismatch);
                }
                repository.id
            } else {
                let [repository_id] = work_item.repository_ids.as_slice() else {
                    return Err(AppError::CheckoutReconciliation {
                        code: "launch_repository_selection_required".to_owned(),
                        message: "the Work item targets multiple repositories; select the launch repository"
                            .to_owned(),
                    });
                };
                *repository_id
            };
            let readiness =
                application
                    .checkout_service()
                    .prepare_work_item(PrepareWorkItemCheckout {
                        work_item_id: work_item.id,
                        repository_id,
                        idempotency_key: format!("{launch_idempotency_key}:checkout"),
                        observed_at: now,
                    })?;
            let request = BeginManagedSessionLaunch {
                owner: HierarchyOwner::WorkItem(work_item.id),
                role: ManagedSessionRole::WorkItemExecution,
                tool,
                mode: ManagedLaunchMode::New,
                checkout_id: readiness.checkout_id,
                working_directory: readiness.path,
                title: work_item.title.clone(),
                terminal_window: Some(format!("workboard-feature-{}", work_item.feature_id)),
                terminal_executable: terminal.unwrap_or_else(default_terminal_executable),
                native_executable: native.unwrap_or_else(|| default_native_executable(tool)),
                idempotency_key: launch_idempotency_key,
                created_at: now,
                expires_at: now + time::Duration::minutes(2),
                resume_context: None,
                initial_prompt: Some(work_item_bootstrap_prompt(work_item.id)),
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
                    work_item.title
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
            let now = time::OffsetDateTime::now_utc();
            application
                .checkout_service()
                .reconcile_registered_checkout(target.checkout.checkout_id, now)?;
            let terminal_window = terminal_window_key(&snapshot, target.owner);
            let context = application.native_sources().resume_context(
                session.id,
                target.checkout.path.clone(),
                target.checkout.title.clone(),
            )?;
            let request = BeginManagedSessionLaunch {
                owner: target.owner,
                role: target.role,
                tool: target.tool,
                mode: ManagedLaunchMode::Resume(target.native_id),
                checkout_id: target.checkout.checkout_id,
                working_directory: target.checkout.path,
                title: target.checkout.title,
                terminal_window: Some(terminal_window),
                terminal_executable: terminal.unwrap_or_else(default_terminal_executable),
                native_executable: native.unwrap_or_else(|| default_native_executable(target.tool)),
                idempotency_key: idempotency_key.unwrap_or_else(new_idempotency_key),
                created_at: now,
                expires_at: now + time::Duration::minutes(2),
                resume_context: Some(context),
                initial_prompt: None,
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
            let readiness = application
                .checkout_service()
                .readiness_for_checkout(checkout.checkout_id)?
                .ok_or_else(|| AppError::CheckoutReconciliation {
                    code: "work_item_checkout_not_isolated".to_owned(),
                    message: "the Work item has no isolated checkout readiness record".to_owned(),
                })?;
            if readiness.owner != HierarchyOwner::WorkItem(work_item.id) {
                return Err(AppError::CheckoutReconciliation {
                    code: "checkout_owner_mismatch".to_owned(),
                    message: "the isolated checkout belongs to a different Work item".to_owned(),
                });
            }
            application
                .checkout_service()
                .reconcile_registered_checkout(
                    checkout.checkout_id,
                    time::OffsetDateTime::now_utc(),
                )?;
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
        Some(Command::Session(SessionArgs {
            command: SessionCommand::RemoveFromRestore { session, reason },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let session = select_session(&snapshot, session.as_deref(), None, cli.json)?.clone();
            application.recovery().remove_from_restore(
                session.id,
                &reason,
                time::OffsetDateTime::now_utc(),
            )?;
            output(
                &session,
                cli.json,
                format!(
                    "Removed {} from the restore set",
                    session.native.native_id()
                ),
            )
        }
        Some(Command::Session(SessionArgs {
            command: SessionCommand::Close { session, reason },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let session = select_session(&snapshot, session.as_deref(), None, cli.json)?.clone();
            let closed = application.session_launch().close(
                session.id,
                &reason,
                time::OffsetDateTime::now_utc(),
                &SystemProcessTerminator,
            )?;
            output(
                &closed,
                cli.json,
                format!("Closed managed session {}", closed.native_id),
            )
        }
        Some(Command::Session(SessionArgs {
            command: SessionCommand::ImportedCandidates { query },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let mut candidates = application.imported_session_candidates(workspace_id)?;
            if let Some(query) = query.as_deref() {
                candidates.retain(|candidate| imported_candidate_matches(candidate, query));
            }
            let human = if candidates.is_empty() {
                "No imported session candidates".to_owned()
            } else {
                candidates
                    .iter()
                    .map(|candidate| {
                        format!(
                            "{} {} {} {}",
                            candidate.session_id,
                            tool_title(candidate.tool),
                            candidate.status,
                            imported_candidate_label(candidate)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            output(&candidates, cli.json, human)
        }
        Some(Command::Session(SessionArgs {
            command: SessionCommand::AdoptImported { session, work_item },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let candidates = application.imported_session_candidates(workspace_id)?;
            let candidate = select_imported_candidate(&candidates, &session, cli.json)?;
            let snapshot = application.snapshot(workspace_id)?;
            let work_item = select_work_item(&snapshot, work_item.as_deref(), cli.json)?;
            application.adopt_imported_session_candidate(
                workspace_id,
                candidate.session_id,
                work_item.id,
                time::OffsetDateTime::now_utc(),
            )?;
            output(
                &serde_json::json!({
                    "sessionId": candidate.session_id,
                    "workItemId": work_item.id,
                    "status": "confirmed",
                }),
                cli.json,
                format!("Adopted {} into {}", candidate.native_id, work_item.title),
            )
        }
        Some(Command::Session(SessionArgs {
            command: SessionCommand::IgnoreImported { session },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let candidates = application.imported_session_candidates(workspace_id)?;
            let candidate = select_imported_candidate(&candidates, &session, cli.json)?;
            application.ignore_imported_session_candidate(workspace_id, candidate.session_id)?;
            output(
                &serde_json::json!({
                    "sessionId": candidate.session_id,
                    "status": "ignored",
                }),
                cli.json,
                format!("Ignored imported session {}", candidate.native_id),
            )
        }
        Some(Command::Workflow(WorkflowArgs { command })) => {
            let workflow_token = workflow_token()?;
            let now = time::OffsetDateTime::now_utc();
            match command {
                WorkflowCommand::ReadHierarchy(arguments) => {
                    let _: serde_json::Value =
                        read_request(&absolute(&current_directory, &arguments.request))?;
                    let hierarchy = application.assigned_hierarchy(&workflow_token, now)?;
                    output(
                        &hierarchy,
                        cli.json,
                        serde_json::to_string_pretty(&hierarchy)?,
                    )
                }
                WorkflowCommand::SubmitFeatureProposal(arguments) => {
                    let request: FeatureProposalRequest =
                        read_request(&absolute(&current_directory, &arguments.request))?;
                    let outcome = application.planning_workflows().submit_proposal(
                        request.feature_id,
                        &workflow_token,
                        request.proposal,
                        &request.idempotency_key,
                        now,
                    )?;
                    output(
                        &outcome,
                        cli.json,
                        format!(
                            "Submitted Feature proposal with {} Work items",
                            outcome.work_item_count
                        ),
                    )
                }
                WorkflowCommand::PublishFeature(arguments) => {
                    let request: FeaturePublicationRequest =
                        read_request(&absolute(&current_directory, &arguments.request))?;
                    let principal = application
                        .workflow_operations()
                        .authenticate(&workflow_token, now)?;
                    if principal.owner != HierarchyOwner::Feature(request.feature_id) {
                        return Err(AppError::WorkflowOperationUnauthorized);
                    }
                    let outcome = application
                        .planning_workflows()
                        .publish_approved(request.feature_id, now)?;
                    output(
                        &outcome,
                        cli.json,
                        format!("Published Feature in commit {}", outcome.commit),
                    )
                }
                WorkflowCommand::CheckpointWorkItem(arguments) => {
                    let request: WorkItemCheckpointRequest =
                        read_request(&absolute(&current_directory, &arguments.request))?;
                    let outcome = application.workflow_operations().checkpoint(
                        &workflow_token,
                        CheckpointWorkItem {
                            work_item_id: request.work_item_id,
                            next_action: request.next_action,
                            summary: request.summary,
                            idempotency_key: request.idempotency_key,
                            recorded_at: now,
                        },
                    )?;
                    output(
                        &outcome,
                        cli.json,
                        format!("Checkpointed Work item {}", outcome.work_item_id),
                    )
                }
                WorkflowCommand::RequestSession(arguments) => {
                    let request: ManagedSessionRequest =
                        read_request(&absolute(&current_directory, &arguments.request))?;
                    let outcome = execute_managed_session_request(
                        &mut application,
                        &workflow_token,
                        request,
                        now,
                    )?;
                    output(
                        &outcome,
                        cli.json,
                        "Managed session request completed".to_owned(),
                    )
                }
            }
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
        Some(Command::Recover(arguments)) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            recovery::execute_recover(&mut application, workspace_id, arguments, cli.json)
        }
        Some(Command::Show) => unreachable!(),
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
        Some(Command::Import(ImportArgs {
            command:
                ImportCommand::ContextCatalogue {
                    command:
                        ContextCatalogueCommand::Apply {
                            preview,
                            repository,
                        },
                },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let repository = select_repository(&snapshot, repository.as_deref(), cli.json)?.clone();
            let preview_path = absolute(&current_directory, &preview);
            let bytes = fs::read(&preview_path).map_err(|source| AppError::StorageIo {
                operation: "reading the legacy import preview",
                path: preview_path.clone(),
                source,
            })?;
            let preview: LegacyImportPreview = serde_json::from_slice(&bytes)?;
            let outcome = application.apply_context_catalogue_import(
                workspace_id,
                repository.id,
                &preview,
            )?;
            output(
                &outcome,
                cli.json,
                format!(
                    "Imported {} sessions, {} checkouts, {} sources, and {} live observations",
                    outcome.native_sessions,
                    outcome.checkouts,
                    outcome.session_sources,
                    outcome.live_observations
                ),
            )
        }
        Some(Command::Import(ImportArgs {
            command:
                ImportCommand::ConcertablePlans {
                    command:
                        ConcertablePlansCommand::Apply {
                            preview,
                            repository,
                        },
                },
        })) => {
            let workspace_id = resolve_workspace(&application, cli.workspace)?;
            let snapshot = application.snapshot(workspace_id)?;
            let repository = select_repository(&snapshot, repository.as_deref(), cli.json)?.clone();
            let preview_path = absolute(&current_directory, &preview);
            let bytes = fs::read(&preview_path).map_err(|source| AppError::PlanningStoreIo {
                operation: "reading the import preview",
                path: preview_path.clone(),
                source,
            })?;
            let preview: ConcertableImportPreview = serde_json::from_slice(&bytes)?;
            let outcome =
                application.apply_concertable_import(workspace_id, repository.id, &preview)?;
            output(
                &outcome,
                cli.json,
                format!(
                    "Imported {} Epics, {} Features, and {} Work items in {}",
                    outcome.epics, outcome.features, outcome.work_items, outcome.planning_commit
                ),
            )
        }
        Some(Command::Import(ImportArgs {
            command:
                ImportCommand::ContextCatalogue {
                    command: ContextCatalogueCommand::Preview { .. },
                },
        }))
        | Some(Command::Import(ImportArgs {
            command:
                ImportCommand::ConcertablePlans {
                    command: ConcertablePlansCommand::Preview { .. },
                },
        })) => unreachable!(),
        Some(Command::Mcp) => unreachable!(),
    }
}

fn run_interactive_board(cli: &Cli) -> Result<(), AppError> {
    let current_directory = std::env::current_dir().map_err(AppError::GitIo)?;
    let database = cli
        .database
        .as_ref()
        .map(|path| absolute(&current_directory, path))
        .map_or_else(default_database_path, Ok)?;
    board::run(client_snapshot(&database, cli.workspace)?)
}

fn client_snapshot(
    database: &Path,
    requested: Option<WorkspaceId>,
) -> Result<workboard_core::WorkspaceSnapshot, AppError> {
    let client = workboard_client::WorkboardClient::discover(database).map_err(client_error)?;
    let workspace_id = match requested {
        Some(id) => workboard_client_protocol::WorkspaceId::from_uuid(*id.as_uuid()),
        None => client.sole_workspace_id().map_err(client_error)?,
    };
    client
        .board_snapshot(workspace_id)
        .map_err(client_error)
        .and_then(core_snapshot)
}

fn client_error(error: workboard_client::ClientError) -> AppError {
    AppError::External {
        code: error.code().to_owned(),
        message: error.to_string(),
    }
}

fn protocol_workspace_id(id: workboard_client_protocol::WorkspaceId) -> WorkspaceId {
    WorkspaceId::from_uuid(*id.as_uuid())
}

fn core_snapshot(
    snapshot: workboard_client_protocol::BoardSnapshot,
) -> Result<workboard_core::WorkspaceSnapshot, AppError> {
    Ok(workboard_core::WorkspaceSnapshot {
        workspace: workboard_core::Workspace {
            id: protocol_workspace_id(snapshot.workspace.id),
            slug: workboard_core::Slug::new(snapshot.workspace.slug)
                .map_err(|error| AppError::Domain(error.to_string()))?,
            title: snapshot.workspace.title,
            planning_store_repository_id: core_repository_id(
                snapshot.workspace.planning_store_repository_id,
            ),
        },
        repositories: snapshot
            .repositories
            .into_iter()
            .map(|repository| {
                Ok(workboard_core::Repository {
                    id: core_repository_id(repository.id),
                    workspace_id: protocol_workspace_id(repository.workspace_id),
                    slug: workboard_core::Slug::new(repository.slug)
                        .map_err(|error| AppError::Domain(error.to_string()))?,
                    title: repository.title,
                    git_common_directory: repository.git_common_directory.into(),
                    default_branch: repository.default_branch,
                    remotes: repository
                        .remotes
                        .into_iter()
                        .map(|remote| workboard_core::RepositoryRemote {
                            name: remote.name,
                            url: remote.url,
                        })
                        .collect(),
                    paths: repository
                        .paths
                        .into_iter()
                        .map(|path| workboard_core::RepositoryPath {
                            id: workboard_core::RepositoryPathId::from_uuid(*path.id.as_uuid()),
                            path: path.path.into(),
                            observed_at: path.observed_at,
                            superseded_at: path.superseded_at,
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?,
        epics: snapshot
            .epics
            .into_iter()
            .map(|epic| {
                Ok(workboard_core::Epic {
                    id: core_epic_id(epic.id),
                    workspace_id: protocol_workspace_id(epic.workspace_id),
                    slug: workboard_core::Slug::new(epic.slug)
                        .map_err(|error| AppError::Domain(error.to_string()))?,
                    title: epic.title,
                    document_id: core_document_id(epic.document_id),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?,
        features: snapshot
            .features
            .into_iter()
            .map(|feature| {
                Ok(workboard_core::Feature {
                    id: core_feature_id(feature.id),
                    epic_id: core_epic_id(feature.epic_id),
                    slug: workboard_core::Slug::new(feature.slug)
                        .map_err(|error| AppError::Domain(error.to_string()))?,
                    title: feature.title,
                    document_id: feature.document_id.map(core_document_id),
                    state: core_workflow_state(feature.state),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?,
        work_items: snapshot
            .work_items
            .into_iter()
            .map(|item| {
                Ok(workboard_core::WorkItem {
                    id: core_work_item_id(item.id),
                    feature_id: core_feature_id(item.feature_id),
                    key: workboard_core::WorkItemKey::new(item.key)
                        .map_err(|error| AppError::Domain(error.to_string()))?,
                    slug: workboard_core::Slug::new(item.slug)
                        .map_err(|error| AppError::Domain(error.to_string()))?,
                    title: item.title,
                    status: core_work_item_status(item.status),
                    document_id: core_document_id(item.document_id),
                    repository_ids: item
                        .repository_ids
                        .into_iter()
                        .map(core_repository_id)
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?,
        documents: snapshot
            .documents
            .into_iter()
            .map(|document| workboard_core::MarkdownDocument {
                id: core_document_id(document.id),
                owner: core_owner(document.owner),
                repository_id: core_repository_id(document.repository_id),
                relative_path: document.relative_path.into(),
                content_hash: document.content_hash,
                observed_commit: document.observed_commit,
            })
            .collect(),
        checkouts: snapshot
            .checkouts
            .into_iter()
            .map(|checkout| workboard_core::Checkout {
                id: core_checkout_id(checkout.id),
                repository_id: core_repository_id(checkout.repository_id),
                git_worktree_identity: checkout.git_worktree_identity,
                branch: checkout.branch,
                head: checkout.head,
                availability: core_checkout_availability(checkout.availability),
                replaces_checkout_id: checkout.replaces_checkout_id.map(core_checkout_id),
                paths: checkout
                    .paths
                    .into_iter()
                    .map(|path| workboard_core::CheckoutPathInterval {
                        id: workboard_core::CheckoutPathId::from_uuid(*path.id.as_uuid()),
                        checkout_id: core_checkout_id(path.checkout_id),
                        path: path.path.into(),
                        observed_from: path.observed_from,
                        observed_until: path.observed_until,
                    })
                    .collect(),
            })
            .collect(),
        effective_checkouts: snapshot
            .effective_checkouts
            .into_iter()
            .map(|checkout| workboard_core::EffectiveCheckout {
                feature_id: core_feature_id(checkout.feature_id),
                work_item_id: checkout.work_item_id.map(core_work_item_id),
                repository_id: core_repository_id(checkout.repository_id),
                checkout_id: core_checkout_id(checkout.checkout_id),
                inherited: checkout.inherited,
            })
            .collect(),
        sessions: snapshot
            .sessions
            .into_iter()
            .map(|session| {
                Ok(workboard_core::NativeSession {
                    id: workboard_core::ConversationId::from_uuid(*session.id.as_uuid()),
                    native: workboard_core::ConversationRef::new(
                        core_provider(session.native.tool),
                        session.native.native_id,
                    )
                    .map_err(|error| AppError::Domain(error.to_string()))?,
                    discovered_at: session.discovered_at,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?,
        associations: snapshot
            .associations
            .into_iter()
            .map(|association| workboard_core::NativeSessionAssociation {
                id: workboard_core::AssociationIntervalId::from_uuid(*association.id.as_uuid()),
                session_id: workboard_core::ConversationId::from_uuid(
                    *association.session_id.as_uuid(),
                ),
                owner: core_owner(association.owner),
                role: core_role(association.role),
                associated_from: association.associated_from,
                associated_until: association.associated_until,
            })
            .collect(),
    })
}

fn core_repository_id(id: workboard_client_protocol::RepositoryId) -> workboard_core::RepositoryId {
    workboard_core::RepositoryId::from_uuid(*id.as_uuid())
}

fn core_epic_id(id: workboard_client_protocol::EpicId) -> workboard_core::EpicId {
    workboard_core::EpicId::from_uuid(*id.as_uuid())
}

fn core_feature_id(id: workboard_client_protocol::FeatureId) -> workboard_core::FeatureId {
    workboard_core::FeatureId::from_uuid(*id.as_uuid())
}

fn core_work_item_id(id: workboard_client_protocol::WorkItemId) -> workboard_core::WorkItemId {
    workboard_core::WorkItemId::from_uuid(*id.as_uuid())
}

fn core_checkout_id(id: workboard_client_protocol::CheckoutId) -> workboard_core::CheckoutId {
    workboard_core::CheckoutId::from_uuid(*id.as_uuid())
}

fn core_document_id(id: workboard_client_protocol::DocumentId) -> workboard_core::DocumentId {
    workboard_core::DocumentId::from_uuid(*id.as_uuid())
}

fn core_owner(owner: workboard_client_protocol::OwnerProjection) -> HierarchyOwner {
    match owner {
        workboard_client_protocol::OwnerProjection::Epic(id) => {
            HierarchyOwner::Epic(core_epic_id(id))
        }
        workboard_client_protocol::OwnerProjection::Feature(id) => {
            HierarchyOwner::Feature(core_feature_id(id))
        }
        workboard_client_protocol::OwnerProjection::WorkItem(id) => {
            HierarchyOwner::WorkItem(core_work_item_id(id))
        }
    }
}

fn core_provider(provider: workboard_client_protocol::Provider) -> Tool {
    match provider {
        workboard_client_protocol::Provider::Claude => Tool::Claude,
        workboard_client_protocol::Provider::Codex => Tool::Codex,
    }
}

fn core_role(role: workboard_client_protocol::ManagedSessionRole) -> ManagedSessionRole {
    match role {
        workboard_client_protocol::ManagedSessionRole::EpicNavigation => {
            ManagedSessionRole::EpicNavigation
        }
        workboard_client_protocol::ManagedSessionRole::FeaturePlanning => {
            ManagedSessionRole::FeaturePlanning
        }
        workboard_client_protocol::ManagedSessionRole::WorkItemExecution => {
            ManagedSessionRole::WorkItemExecution
        }
        workboard_client_protocol::ManagedSessionRole::Debugging => ManagedSessionRole::Debugging,
        workboard_client_protocol::ManagedSessionRole::Review => ManagedSessionRole::Review,
    }
}

fn core_work_item_status(
    status: workboard_client_protocol::WorkItemStatus,
) -> workboard_core::WorkItemStatus {
    match status {
        workboard_client_protocol::WorkItemStatus::Backlog => {
            workboard_core::WorkItemStatus::Backlog
        }
        workboard_client_protocol::WorkItemStatus::Ready => workboard_core::WorkItemStatus::Ready,
        workboard_client_protocol::WorkItemStatus::InProgress => {
            workboard_core::WorkItemStatus::InProgress
        }
        workboard_client_protocol::WorkItemStatus::Blocked => {
            workboard_core::WorkItemStatus::Blocked
        }
        workboard_client_protocol::WorkItemStatus::Review => workboard_core::WorkItemStatus::Review,
        workboard_client_protocol::WorkItemStatus::Done => workboard_core::WorkItemStatus::Done,
        workboard_client_protocol::WorkItemStatus::Cancelled => {
            workboard_core::WorkItemStatus::Cancelled
        }
    }
}

fn core_checkout_availability(
    availability: workboard_client_protocol::CheckoutAvailability,
) -> CheckoutAvailability {
    match availability {
        workboard_client_protocol::CheckoutAvailability::Available => {
            CheckoutAvailability::Available
        }
        workboard_client_protocol::CheckoutAvailability::Missing => CheckoutAvailability::Missing,
        workboard_client_protocol::CheckoutAvailability::Deleted => CheckoutAvailability::Deleted,
        workboard_client_protocol::CheckoutAvailability::Replaced => CheckoutAvailability::Replaced,
    }
}

fn core_workflow_state(
    state: workboard_client_protocol::WorkflowState,
) -> workboard_core::WorkflowState {
    match state {
        workboard_client_protocol::WorkflowState::Draft => workboard_core::WorkflowState::Draft,
        workboard_client_protocol::WorkflowState::WorktreePending => {
            workboard_core::WorkflowState::WorktreePending
        }
        workboard_client_protocol::WorkflowState::PlanningLaunchPending => {
            workboard_core::WorkflowState::PlanningLaunchPending
        }
        workboard_client_protocol::WorkflowState::PlanningActive => {
            workboard_core::WorkflowState::PlanningActive
        }
        workboard_client_protocol::WorkflowState::ProposalReady => {
            workboard_core::WorkflowState::ProposalReady
        }
        workboard_client_protocol::WorkflowState::AwaitingApproval => {
            workboard_core::WorkflowState::AwaitingApproval
        }
        workboard_client_protocol::WorkflowState::Publishing => {
            workboard_core::WorkflowState::Publishing
        }
        workboard_client_protocol::WorkflowState::Planned => workboard_core::WorkflowState::Planned,
        workboard_client_protocol::WorkflowState::WorkItemLaunchPending => {
            workboard_core::WorkflowState::WorkItemLaunchPending
        }
        workboard_client_protocol::WorkflowState::WorkItemActive => {
            workboard_core::WorkflowState::WorkItemActive
        }
        workboard_client_protocol::WorkflowState::ReconciliationRequired => {
            workboard_core::WorkflowState::ReconciliationRequired
        }
        workboard_client_protocol::WorkflowState::Blocked => workboard_core::WorkflowState::Blocked,
        workboard_client_protocol::WorkflowState::Paused => workboard_core::WorkflowState::Paused,
        workboard_client_protocol::WorkflowState::Completed => {
            workboard_core::WorkflowState::Completed
        }
        workboard_client_protocol::WorkflowState::Cancelled => {
            workboard_core::WorkflowState::Cancelled
        }
    }
}

fn output<T: Serialize>(value: &T, json: bool, human: String) -> Result<String, AppError> {
    if json {
        serde_json::to_string_pretty(value).map_err(Into::into)
    } else {
        Ok(human)
    }
}

fn terminal_window_key(
    snapshot: &workboard_core::WorkspaceSnapshot,
    owner: HierarchyOwner,
) -> String {
    match owner {
        HierarchyOwner::Epic(id) => format!("workboard-epic-{id}"),
        HierarchyOwner::Feature(id) => format!("workboard-feature-{id}"),
        HierarchyOwner::WorkItem(id) => snapshot
            .work_items
            .iter()
            .find(|item| item.id == id)
            .map_or_else(
                || format!("workboard-work-item-{id}"),
                |item| format!("workboard-feature-{}", item.feature_id),
            ),
    }
}

fn resolve_workspace(
    application: &WorkboardApplication,
    requested: Option<WorkspaceId>,
) -> Result<WorkspaceId, AppError> {
    requested.map_or_else(|| application.sole_workspace_id(), Ok)
}

fn execute_epic_continue(
    application: &mut WorkboardApplication,
    workspace_id: WorkspaceId,
    arguments: EpicContinueArgs,
    json: bool,
) -> Result<String, AppError> {
    let snapshot = application.snapshot(workspace_id)?;
    let epic = select_epic(&snapshot, arguments.epic.as_deref(), json)?.clone();
    let repository = select_repository(&snapshot, arguments.repository.as_deref(), json)?.clone();
    let tool = Tool::from(arguments.tool);
    let now = time::OffsetDateTime::now_utc();
    ensure_integration(application, tool, now)?;
    let checkout = application.ensure_repository_checkout(repository.id, now)?;
    let prompt = format!(
        "Use the installed Agent Workboard workflow to continue Epic {} ({}). Read the assigned hierarchy, collaborate with the user to choose the next Feature, and hand implementation planning to workboard feature create. Do not publish or edit planning documents directly from this Epic navigation session.",
        epic.title, epic.id
    );
    let prepared = application
        .session_launch()
        .begin(BeginManagedSessionLaunch {
            owner: HierarchyOwner::Epic(epic.id),
            role: ManagedSessionRole::EpicNavigation,
            tool,
            mode: ManagedLaunchMode::New,
            checkout_id: checkout.checkout_id,
            working_directory: checkout.path,
            title: epic.title.clone(),
            terminal_window: Some(format!("workboard-epic-{}", epic.id)),
            terminal_executable: arguments
                .terminal
                .unwrap_or_else(default_terminal_executable),
            native_executable: arguments
                .native
                .unwrap_or_else(|| default_native_executable(tool)),
            idempotency_key: arguments
                .idempotency_key
                .unwrap_or_else(new_idempotency_key),
            created_at: now,
            expires_at: now + time::Duration::minutes(2),
            resume_context: None,
            initial_prompt: Some(prompt),
        })?;
    application
        .session_launch()
        .execute(&prepared, &SystemLaunchExecutor)?;
    let binding = await_binding(application, prepared.intent_id)?;
    output(
        &binding,
        json,
        format!(
            "Launched and bound {} Epic navigator for {}",
            tool_title(tool),
            epic.title
        ),
    )
}

fn execute_feature_create(
    application: &mut WorkboardApplication,
    current_directory: &Path,
    workspace_id: WorkspaceId,
    arguments: FeatureCreateArgs,
    json: bool,
) -> Result<String, AppError> {
    let snapshot = application.snapshot(workspace_id)?;
    let epic = select_epic(&snapshot, arguments.epic.as_deref(), json)?.clone();
    let repository = select_repository(&snapshot, arguments.repository.as_deref(), json)?.clone();
    let tool = Tool::from(arguments.tool);
    let workboard_executable = std::env::current_exe().map_err(AppError::GitIo)?;
    let status = application.integrations().execute(
        IntegrationRequest {
            tool,
            native_home: default_integration_home(tool)?,
            workboard_executable,
            operation: IntegrationOperation::Status,
            preview_operation: None,
            confirmation: None,
        },
        time::OffsetDateTime::now_utc(),
    )?;
    if !matches!(
        status,
        IntegrationResponse::Status { status }
            if status.state == IntegrationState::Installed && status.enabled_in_workboard
    ) {
        return Err(AppError::External {
            code: "integration_required".to_owned(),
            message: format!(
                "{} integration must be installed and healthy before managed planning",
                tool_title(tool)
            ),
        });
    }

    let slug = parse_or_derive_slug(arguments.slug.as_deref(), &arguments.title)?;
    let idempotency_key = arguments
        .idempotency_key
        .unwrap_or_else(new_idempotency_key);
    let now = time::OffsetDateTime::now_utc();
    let draft = application
        .planning_workflows()
        .create_feature(CreateFeaturePlanning {
            epic_id: epic.id,
            repository_id: repository.id,
            slug: slug.clone(),
            title: arguments.title,
            idempotency_key: format!("{idempotency_key}:feature"),
            created_at: now,
        })?;
    let repository_path = repository
        .paths
        .iter()
        .find(|path| path.superseded_at.is_none())
        .map(|path| path.path.clone())
        .ok_or(AppError::ResumeRepositoryMismatch)?;
    let worktree = arguments
        .worktree
        .map(|path| absolute(current_directory, &path))
        .unwrap_or_else(|| default_feature_worktree(&repository_path, &slug));
    let parent = worktree
        .parent()
        .ok_or_else(|| AppError::WorktreePathNotAbsolute(worktree.clone()))?;
    fs::create_dir_all(parent).map_err(AppError::GitIo)?;
    let branch = arguments
        .branch
        .unwrap_or_else(|| format!("feature/{slug}"));
    let base = arguments.base.unwrap_or_else(|| {
        repository
            .default_branch
            .clone()
            .unwrap_or_else(|| "main".to_owned())
    });
    let checkout = application
        .checkout_service()
        .prepare_feature(PrepareFeatureCheckout {
            feature_id: draft.feature_id,
            repository_id: repository.id,
            target: worktree,
            branch,
            create_branch: true,
            start_point: base,
            idempotency_key: format!("{idempotency_key}:checkout"),
            observed_at: now,
        })?;
    let draft = application.planning_workflows().mark_launch_pending(
        draft.feature_id,
        checkout.checkout_id,
        now,
    )?;
    let prepared = application
        .session_launch()
        .begin(BeginManagedSessionLaunch {
            owner: HierarchyOwner::Feature(draft.feature_id),
            role: ManagedSessionRole::FeaturePlanning,
            tool,
            mode: ManagedLaunchMode::New,
            checkout_id: checkout.checkout_id,
            working_directory: checkout.path,
            title: draft.title.clone(),
            terminal_window: Some(format!("workboard-feature-{}", draft.feature_id)),
            terminal_executable: arguments
                .terminal
                .unwrap_or_else(default_terminal_executable),
            native_executable: arguments
                .native
                .unwrap_or_else(|| default_native_executable(tool)),
            idempotency_key: format!("{idempotency_key}:launch"),
            created_at: now,
            expires_at: now + time::Duration::minutes(2),
            resume_context: None,
            initial_prompt: Some(planner_bootstrap_prompt(&draft)),
        })?;
    application
        .session_launch()
        .execute(&prepared, &SystemLaunchExecutor)?;
    let binding = await_binding(application, prepared.intent_id)?;
    output(
        &binding,
        json,
        format!(
            "Launched and bound {} planner for {} ({})",
            tool_title(tool),
            draft.title,
            draft.feature_id
        ),
    )
}

fn default_feature_worktree(repository: &Path, feature: &Slug) -> PathBuf {
    let name = repository
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("repository");
    repository
        .parent()
        .unwrap_or(repository)
        .join(format!("{name}.worktrees"))
        .join(feature.as_str())
}

fn select_epic<'a>(
    snapshot: &'a workboard_core::WorkspaceSnapshot,
    query: Option<&str>,
    structured: bool,
) -> Result<&'a Epic, AppError> {
    let candidates = snapshot.epics.iter().map(|epic| SelectionCandidate {
        id: epic.id.to_string(),
        key: Some(epic.slug.to_string()),
        label: epic.title.clone(),
        metadata: "Epic".to_owned(),
    });
    let candidate = select_candidate("Epic", query, candidates.collect(), structured)?;
    snapshot
        .epics
        .iter()
        .find(|epic| epic.id.to_string() == candidate.id)
        .ok_or_else(|| AppError::Domain("selected Epic is unavailable".to_owned()))
}

fn select_repository<'a>(
    snapshot: &'a workboard_core::WorkspaceSnapshot,
    query: Option<&str>,
    structured: bool,
) -> Result<&'a Repository, AppError> {
    let candidates = snapshot
        .repositories
        .iter()
        .filter(|repository| repository.id != snapshot.workspace.planning_store_repository_id)
        .map(|repository| SelectionCandidate {
            id: repository.id.to_string(),
            key: Some(repository.slug.to_string()),
            label: repository.title.clone(),
            metadata: repository
                .default_branch
                .clone()
                .unwrap_or_else(|| "detached".to_owned()),
        })
        .collect();
    let candidate = select_candidate("repository", query, candidates, structured)?;
    snapshot
        .repositories
        .iter()
        .find(|repository| repository.id.to_string() == candidate.id)
        .ok_or(AppError::ResumeRepositoryMismatch)
}

fn select_imported_candidate<'a>(
    candidates: &'a [ImportedSessionCandidate],
    query: &str,
    structured: bool,
) -> Result<&'a ImportedSessionCandidate, AppError> {
    let options = candidates
        .iter()
        .filter(|candidate| candidate.status == "unassigned")
        .map(|candidate| SelectionCandidate {
            id: candidate.session_id.to_string(),
            key: Some(candidate.native_id.clone()),
            label: imported_candidate_label(candidate),
            metadata: [
                Some(tool_title(candidate.tool).to_owned()),
                candidate.legacy_workstream_title.clone(),
                candidate
                    .observed_cwd
                    .as_deref()
                    .map(|path| path.to_string_lossy().into_owned()),
                candidate.first_prompt_preview.clone(),
                candidate.last_prompt_preview.clone(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" | "),
        })
        .collect();
    let selected = select_candidate("imported session", Some(query), options, structured)?;
    candidates
        .iter()
        .find(|candidate| candidate.session_id.to_string() == selected.id)
        .ok_or(AppError::ConversationNotFound)
}

fn imported_candidate_label(candidate: &ImportedSessionCandidate) -> String {
    let value = candidate
        .native_title
        .as_deref()
        .or(candidate.legacy_workstream_title.as_deref())
        .or(candidate.last_prompt_preview.as_deref())
        .unwrap_or(&candidate.native_id);
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= 120 {
        compact
    } else {
        format!("{}…", compact.chars().take(119).collect::<String>())
    }
}

fn imported_candidate_matches(candidate: &ImportedSessionCandidate, query: &str) -> bool {
    let query = query.to_lowercase();
    [
        Some(candidate.session_id.to_string()),
        Some(candidate.native_id.clone()),
        candidate.native_title.clone(),
        candidate.legacy_workstream_id.clone(),
        candidate.legacy_workstream_title.clone(),
        candidate.first_prompt_preview.clone(),
        candidate.last_prompt_preview.clone(),
        candidate
            .observed_cwd
            .as_deref()
            .map(|path| path.to_string_lossy().into_owned()),
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_lowercase().contains(&query))
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

fn select_available_checkout<'a>(
    snapshot: &'a workboard_core::WorkspaceSnapshot,
    query: &str,
    structured: bool,
) -> Result<&'a Checkout, AppError> {
    let candidates = snapshot
        .checkouts
        .iter()
        .filter(|checkout| checkout.availability == CheckoutAvailability::Available)
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
        .ok_or(AppError::ResumeCheckoutNotScanned)
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

#[cfg(windows)]
fn codex_app_executable(local_app_data: &Path) -> Option<PathBuf> {
    let bin = local_app_data.join("OpenAI").join("Codex").join("bin");
    fs::read_dir(bin)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("codex.exe"))
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let modified = path.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

#[cfg(windows)]
fn default_codex_executable() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .and_then(|local_app_data| codex_app_executable(Path::new(&local_app_data)))
        .unwrap_or_else(|| PathBuf::from("codex"))
}

#[cfg(not(windows))]
fn default_codex_executable() -> PathBuf {
    PathBuf::from("codex")
}

fn default_native_executable(tool: Tool) -> PathBuf {
    match tool {
        Tool::Claude => PathBuf::from("claude"),
        Tool::Codex => default_codex_executable(),
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

fn workflow_token() -> Result<String, AppError> {
    std::env::var(workboard_core::WORKBOARD_WORKFLOW_TOKEN_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(AppError::WorkflowOperationUnauthorized)
}

fn read_request<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AppError> {
    const MAX_REQUEST_BYTES: u64 = 2 * 1024 * 1024;
    let metadata = fs::metadata(path).map_err(|source| AppError::PlanningStoreIo {
        operation: "reading workflow request metadata",
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > MAX_REQUEST_BYTES {
        return Err(AppError::PlanningDocumentInvalid(
            "workflow request exceeds 2 MiB".to_owned(),
        ));
    }
    let bytes = fs::read(path).map_err(|source| AppError::PlanningStoreIo {
        operation: "reading workflow request",
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(Into::into)
}

fn execute_managed_session_request(
    application: &mut WorkboardApplication,
    workflow_token: &str,
    request: ManagedSessionRequest,
    now: time::OffsetDateTime,
) -> Result<serde_json::Value, AppError> {
    ensure_integration(application, request.tool, now)?;
    let hierarchy = application.assigned_hierarchy(workflow_token, now)?;
    let work_item = hierarchy
        .snapshot
        .work_items
        .iter()
        .find(|item| item.id == request.work_item_id)
        .ok_or(AppError::WorkItemNotFound)?;
    let repository_id = if let Some(repository_id) = request.repository_id {
        if !work_item.repository_ids.contains(&repository_id) {
            return Err(AppError::WorkItemRepositoryMismatch);
        }
        repository_id
    } else {
        let [repository_id] = work_item.repository_ids.as_slice() else {
            return Err(AppError::CheckoutReconciliation {
                code: "launch_repository_selection_required".to_owned(),
                message:
                    "the Work item targets multiple repositories; select the launch repository"
                        .to_owned(),
            });
        };
        *repository_id
    };
    let terminal_window = terminal_window_key(
        &hierarchy.snapshot,
        HierarchyOwner::WorkItem(request.work_item_id),
    );
    let idempotency_key = request.idempotency_key.clone();
    let terminal = request.terminal.unwrap_or_else(default_terminal_executable);
    let native = request
        .native
        .unwrap_or_else(|| default_native_executable(request.tool));
    let operation_request = RequestManagedSession {
        work_item_id: request.work_item_id,
        repository_id,
        tool: request.tool,
        idempotency_key,
        requested_at: now,
    };
    if let Some(existing) = application
        .workflow_operations()
        .existing_session_request(workflow_token, &operation_request)?
    {
        if existing.status == "bound" {
            return serde_json::to_value(&existing).map_err(Into::into);
        }
        if existing.status != "pending" {
            return Err(AppError::External {
                code: "session_request_in_progress".to_owned(),
                message: format!(
                    "managed session request {} is {}",
                    existing.request_id, existing.status
                ),
            });
        }
    }
    application
        .checkout_service()
        .prepare_work_item(PrepareWorkItemCheckout {
            work_item_id: request.work_item_id,
            repository_id,
            idempotency_key: format!("{}:checkout", operation_request.idempotency_key),
            observed_at: now,
        })?;
    let requested = application
        .workflow_operations()
        .request_session(workflow_token, operation_request)?;
    if requested.status != "pending" {
        return Err(AppError::External {
            code: "session_request_in_progress".to_owned(),
            message: format!(
                "managed session request {} is {}",
                requested.request_id, requested.status
            ),
        });
    }
    let prepared = application
        .session_launch()
        .begin(BeginManagedSessionLaunch {
            owner: HierarchyOwner::WorkItem(requested.work_item_id),
            role: ManagedSessionRole::WorkItemExecution,
            tool: requested.tool,
            mode: ManagedLaunchMode::New,
            checkout_id: requested.checkout_id,
            working_directory: requested.working_directory.clone(),
            title: requested.title.clone(),
            terminal_window: Some(terminal_window),
            terminal_executable: terminal,
            native_executable: native,
            idempotency_key: format!("{}:launch", requested.request_id),
            created_at: now,
            expires_at: now + time::Duration::minutes(2),
            resume_context: None,
            initial_prompt: Some(work_item_bootstrap_prompt(requested.work_item_id)),
        })?;
    application
        .session_launch()
        .execute(&prepared, &SystemLaunchExecutor)?;
    application
        .workflow_operations()
        .record_session_launch(requested.request_id, prepared.intent_id)?;
    let binding = await_binding(application, prepared.intent_id)?;
    application
        .workflow_operations()
        .record_session_binding(requested.request_id)?;
    Ok(serde_json::json!({
        "request": requested,
        "binding": binding,
    }))
}

fn ensure_integration(
    application: &mut WorkboardApplication,
    tool: Tool,
    now: time::OffsetDateTime,
) -> Result<(), AppError> {
    let response = application.integrations().execute(
        IntegrationRequest {
            tool,
            native_home: default_integration_home(tool)?,
            workboard_executable: std::env::current_exe().map_err(AppError::GitIo)?,
            operation: IntegrationOperation::Status,
            preview_operation: None,
            confirmation: None,
        },
        now,
    )?;
    if matches!(
        response,
        IntegrationResponse::Status { status }
            if status.state == IntegrationState::Installed && status.enabled_in_workboard
    ) {
        Ok(())
    } else {
        Err(AppError::External {
            code: "integration_required".to_owned(),
            message: format!(
                "{} integration must be installed and healthy before managed launch",
                tool_title(tool)
            ),
        })
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
    use std::fs;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::process::Command;

    use clap::Parser;
    use rusqlite::params;
    use tempfile::TempDir;
    use workboard_application::workspace::WorkboardApplication;
    use workboard_core::{RepositoryId, WorkspaceId};
    use workboard_daemon::{DaemonServer, EndpointRegistration};

    use crate::selector::SelectionCandidate;

    use super::{
        Cli, Command as CliCommand, SessionCommand, codex_app_executable, execute_from,
        select_candidate, slugify,
    };

    #[test]
    fn derives_safe_slugs() {
        assert_eq!(slugify("Venue Availability API"), "venue-availability-api");
        assert_eq!(slugify("  Mixed___punctuation  "), "mixed-punctuation");
    }

    #[test]
    fn migrated_bare_board_and_show_reads_require_the_daemon_client() {
        let directory = TempDir::new().expect("temporary directory");
        let database = directory.path().join("workboard.sqlite");
        let application = WorkboardApplication::open(&database).expect("open application");
        let workspace_id = WorkspaceId::generate();
        let repository_id = RepositoryId::generate();
        let mut connection = rusqlite::Connection::open(&database).expect("open fixture database");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("enable foreign keys");
        let transaction = connection.transaction().expect("fixture transaction");
        transaction
            .execute(
                "INSERT INTO workspaces (id, slug, title, planning_store_repository_id, created_at)
                 VALUES (?1, 'workspace', 'Workspace', ?2, '2026-08-30T00:00:00Z')",
                params![workspace_id.to_string(), repository_id.to_string()],
            )
            .expect("insert Workspace");
        transaction
            .execute(
                "INSERT INTO repositories (
                     id, workspace_id, slug, title, git_common_directory, default_branch,
                     is_planning_store, created_at
                 ) VALUES (
                     ?1, ?2, 'planning', 'Planning', 'C:/planning/.git', 'main', 1,
                     '2026-08-30T00:00:00Z'
                 )",
                params![repository_id.to_string(), workspace_id.to_string()],
            )
            .expect("insert repository");
        transaction.commit().expect("commit fixture");
        drop(connection);
        let server = DaemonServer::start_application(
            application,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            "opaque-token",
        )
        .expect("start daemon");
        let registration =
            EndpointRegistration::claim(&database, &server.descriptor()).expect("publish endpoint");
        let database_text = database.to_str().expect("database path");
        let bare =
            execute_from(["workboard", "--database", database_text, "--json"]).expect("bare board");
        let shown = execute_from(["workboard", "--database", database_text, "--json", "show"])
            .expect("structured show");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&bare).expect("bare JSON"),
            serde_json::from_str::<serde_json::Value>(&shown).expect("show JSON")
        );
        drop(server);
        let error = execute_from(["workboard", "--database", database_text, "--json", "show"])
            .expect_err("no storage fallback");
        assert_eq!(error.code(), "daemon_io");
        drop(registration);
    }

    #[cfg(windows)]
    #[test]
    fn resolves_the_codex_app_managed_cli() {
        let directory = TempDir::new().expect("temporary local app data");
        let executable = directory
            .path()
            .join("OpenAI")
            .join("Codex")
            .join("bin")
            .join("current")
            .join("codex.exe");
        fs::create_dir_all(executable.parent().expect("Codex bin directory"))
            .expect("create Codex bin directory");
        fs::write(&executable, []).expect("create Codex executable");
        assert_eq!(codex_app_executable(directory.path()), Some(executable));
    }

    #[test]
    fn invalid_command_returns_a_typed_error() {
        let error =
            execute_from(["workboard", "epic", "create"]).expect_err("missing title should fail");
        assert_eq!(error.code(), "domain");
    }

    #[test]
    fn show_command_accepts_the_legacy_snapshot_alias() {
        for command in ["show", "snapshot"] {
            let cli = Cli::try_parse_from(["workboard", command]).expect("show command");
            assert!(matches!(cli.command, Some(CliCommand::Show)));
        }
    }

    #[test]
    fn recovery_commands_parse_period_selection_confirmation_and_removal() {
        let cli = Cli::try_parse_from([
            "workboard",
            "recover",
            "--since",
            "yesterday",
            "--dry-run",
            "--session",
            "thread-one",
        ])
        .expect("recover command");
        let Some(CliCommand::Recover(arguments)) = cli.command else {
            panic!("expected recover command");
        };
        assert_eq!(arguments.since.as_deref(), Some("yesterday"));
        assert!(arguments.dry_run);
        assert_eq!(arguments.sessions, ["thread-one"]);

        let cli = Cli::try_parse_from([
            "workboard",
            "session",
            "remove-from-restore",
            "thread-one",
            "--reason",
            "completed",
        ])
        .expect("remove restore command");
        let Some(CliCommand::Session(arguments)) = cli.command else {
            panic!("expected session command");
        };
        assert!(matches!(
            arguments.command,
            SessionCommand::RemoveFromRestore { session, reason }
                if session.as_deref() == Some("thread-one") && reason == "completed"
        ));

        let cli = Cli::try_parse_from([
            "workboard",
            "session",
            "close",
            "thread-one",
            "--reason",
            "completed",
        ])
        .expect("close managed session command");
        let Some(CliCommand::Session(arguments)) = cli.command else {
            panic!("expected session command");
        };
        assert!(matches!(
            arguments.command,
            SessionCommand::Close { session, reason }
                if session.as_deref() == Some("thread-one") && reason == "completed"
        ));
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
