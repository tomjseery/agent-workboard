use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use uuid::Uuid;
use workboard_application::workspace::WorkboardApplication;
use workboard_daemon::{DaemonServer, EndpointRegistration, WatchConfig};

#[derive(Parser)]
struct Arguments {
    #[arg(long, env = "WORKBOARD_DATABASE")]
    database: PathBuf,
    #[arg(long, default_value = "127.0.0.1:0")]
    address: SocketAddr,
    #[arg(long)]
    claude: Option<PathBuf>,
    #[arg(long)]
    codex: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = Arguments::parse();
    let registration = EndpointRegistration::acquire(&arguments.database)?;
    let application = WorkboardApplication::open(&arguments.database)?;
    let mut server = DaemonServer::start_application(
        application,
        arguments.address,
        Uuid::new_v4().to_string(),
    )?;
    registration.publish(&server.descriptor())?;
    if arguments.claude.is_some() || arguments.codex.is_some() {
        server.enable_watcher(WatchConfig::new(arguments.claude, arguments.codex))?;
    }
    server.wait()?;
    Ok(())
}
