use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use clap::Parser;
use directories::{ProjectDirs, UserDirs};
use serde_json::{Value, json};
use time::OffsetDateTime;
use uuid::Uuid;
use workboard_application::native_sources::{NativeRefreshOutcome, RefreshNativeSources};
use workboard_application::workspace::WorkboardApplication;
use workboard_core::{PRODUCT_NAME, Tool};
use workboard_daemon::{
    DaemonServer, EndpointRegistration, RemoteError, WatchConfig, WriteCommand,
};

#[derive(Debug, Parser)]
#[command(
    name = "workboard-daemon",
    version,
    about = "Agent Workboard background refresh daemon"
)]
struct Cli {
    #[arg(long, env = "WORKBOARD_DATABASE")]
    database: Option<PathBuf>,
    #[arg(long)]
    claude_root: Option<PathBuf>,
    #[arg(long)]
    codex_root: Option<PathBuf>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let current_directory = env::current_dir()?;
    let database = cli
        .database
        .map(|path| absolute(&current_directory, path))
        .map_or_else(default_database_path, Ok)?;
    let claude_root = cli
        .claude_root
        .map(|path| absolute(&current_directory, path))
        .or_else(|| default_native_root(Tool::Claude));
    let codex_root = cli
        .codex_root
        .map(|path| absolute(&current_directory, path))
        .or_else(|| default_native_root(Tool::Codex));
    let mut application = WorkboardApplication::open(&database)?;
    let mut server = DaemonServer::start(
        move |command| handle_command(&mut application, command),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        Uuid::new_v4().to_string(),
    )?;
    let registration = EndpointRegistration::claim(&database, &server.descriptor())?;
    server.enable_watcher(WatchConfig::new(claude_root, codex_root))?;
    let result = server.wait();
    drop(registration);
    result.map_err(Into::into)
}

fn handle_command(
    application: &mut WorkboardApplication,
    command: WriteCommand,
) -> Result<Value, RemoteError> {
    match command {
        WriteCommand::RefreshNativeSessions {
            claude_root,
            codex_root,
        } => {
            let observed_at = OffsetDateTime::now_utc();
            let claude = refresh(application, Tool::Claude, claude_root, observed_at)?;
            let codex = refresh(application, Tool::Codex, codex_root, observed_at)?;
            Ok(json!({ "claude": claude, "codex": codex }))
        }
        WriteCommand::Ping => Ok(json!({ "status": "ready" })),
    }
}

fn refresh(
    application: &mut WorkboardApplication,
    tool: Tool,
    root: Option<PathBuf>,
    observed_at: OffsetDateTime,
) -> Result<NativeRefreshOutcome, RemoteError> {
    let mut roots = root.into_iter().collect::<Vec<_>>();
    roots.extend(
        application
            .managed_transcript_roots(tool)
            .map_err(remote_error)?,
    );
    let mut total = NativeRefreshOutcome {
        tool,
        inventory_count: 0,
        source_count: 0,
        conversation_count: 0,
        failures: Vec::new(),
    };
    for root in roots.into_iter().filter(|root| root.is_dir()) {
        let outcome = application
            .native_sources()
            .refresh(RefreshNativeSources {
                tool,
                root,
                observed_at,
            })
            .map_err(remote_error)?;
        total.inventory_count += outcome.inventory_count;
        total.source_count += outcome.source_count;
        total.conversation_count += outcome.conversation_count;
        total.failures.extend(outcome.failures);
    }
    Ok(total)
}

fn remote_error(error: workboard_application::AppError) -> RemoteError {
    RemoteError {
        code: error.code().to_owned(),
        message: error.to_string(),
    }
}

fn default_database_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    ProjectDirs::from("dev", PRODUCT_NAME, PRODUCT_NAME)
        .map(|directories| directories.data_local_dir().join("workboard.sqlite"))
        .ok_or_else(|| "the Agent Workboard data directory is unavailable".into())
}

fn default_native_root(tool: Tool) -> Option<PathBuf> {
    let environment = match tool {
        Tool::Claude => "CLAUDE_CONFIG_DIR",
        Tool::Codex => "CODEX_HOME",
    };
    let directory = match tool {
        Tool::Claude => "projects",
        Tool::Codex => "sessions",
    };
    env::var_os(environment)
        .map(PathBuf::from)
        .or_else(|| {
            UserDirs::new().map(|directories| match tool {
                Tool::Claude => directories.home_dir().join(".claude"),
                Tool::Codex => directories.home_dir().join(".codex"),
            })
        })
        .map(|root| root.join(directory))
}

fn absolute(current_directory: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        current_directory.join(path)
    }
}
