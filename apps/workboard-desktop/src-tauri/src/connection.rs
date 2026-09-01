use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use directories::ProjectDirs;
use serde::Serialize;
use workboard_client::{ClientError, WorkboardClient, endpoint_path, read_descriptor};

use crate::types::{BootstrapState, BridgeError};

const CONNECT_ATTEMPTS: usize = 40;
const CONNECT_DELAY: Duration = Duration::from_millis(50);

pub(crate) trait DaemonStarter: Send + Sync {
    fn start(&self, database: &Path) -> Result<(), BridgeError>;
}

struct ProcessDaemonStarter {
    executable: PathBuf,
    child: Mutex<Option<Child>>,
}

impl ProcessDaemonStarter {
    fn system() -> Result<Self, BridgeError> {
        let executable = match std::env::var_os("WORKBOARD_DAEMON") {
            Some(path) => PathBuf::from(path),
            None => {
                let mut path = std::env::current_exe().map_err(|_| BridgeError::disconnected())?;
                path.set_file_name(if cfg!(windows) {
                    "workboard.exe"
                } else {
                    "workboard"
                });
                path
            }
        };
        Ok(Self {
            executable,
            child: Mutex::new(None),
        })
    }
}

impl DaemonStarter for ProcessDaemonStarter {
    fn start(&self, database: &Path) -> Result<(), BridgeError> {
        let mut command = Command::new(&self.executable);
        command
            .arg("daemon")
            .arg("--database")
            .arg(database)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let child = command.spawn().map_err(|_| BridgeError::disconnected())?;
        *self.child.lock().expect("daemon child lock") = Some(child);
        Ok(())
    }
}

pub(crate) struct ConnectedClient {
    pub client: WorkboardClient,
    credential: String,
}

impl ConnectedClient {
    fn connect(database: &Path) -> Result<Self, ClientError> {
        let descriptor = read_descriptor(&endpoint_path(database))?;
        let credential = descriptor.token.clone();
        let client = WorkboardClient::connect(descriptor)?;
        Ok(Self { client, credential })
    }

    pub fn ensure_safe<T: Serialize>(&self, value: &T) -> Result<(), BridgeError> {
        let bytes = serde_json::to_vec(value).map_err(|_| BridgeError::unsafe_payload())?;
        if !self.credential.is_empty()
            && bytes
                .windows(self.credential.len())
                .any(|window| window == self.credential.as_bytes())
        {
            return Err(BridgeError::unsafe_payload());
        }
        Ok(())
    }

    pub fn state(&self) -> BootstrapState {
        if self
            .client
            .handshake()
            .command_capabilities
            .iter()
            .any(|capability| capability.available)
        {
            BootstrapState::Ready
        } else {
            BootstrapState::ReadOnly
        }
    }
}

#[derive(Clone)]
pub(crate) struct ConnectionManager {
    inner: Arc<ConnectionManagerInner>,
}

struct ConnectionManagerInner {
    database: PathBuf,
    starter: Arc<dyn DaemonStarter>,
    connected: Mutex<Option<Arc<ConnectedClient>>>,
}

impl ConnectionManager {
    pub fn system() -> Result<Self, BridgeError> {
        let database = match std::env::var_os("WORKBOARD_DATABASE") {
            Some(path) => PathBuf::from(path),
            None => ProjectDirs::from("dev", "Agent Workboard", "Agent Workboard")
                .map(|directories| directories.data_local_dir().join("workboard.sqlite"))
                .ok_or_else(BridgeError::disconnected)?,
        };
        Ok(Self::new(
            database,
            Arc::new(ProcessDaemonStarter::system()?),
        ))
    }

    #[cfg(test)]
    pub fn new(database: PathBuf, starter: Arc<dyn DaemonStarter>) -> Self {
        Self {
            inner: Arc::new(ConnectionManagerInner {
                database,
                starter,
                connected: Mutex::new(None),
            }),
        }
    }

    #[cfg(not(test))]
    fn new(database: PathBuf, starter: Arc<dyn DaemonStarter>) -> Self {
        Self {
            inner: Arc::new(ConnectionManagerInner {
                database,
                starter,
                connected: Mutex::new(None),
            }),
        }
    }

    pub fn client(&self) -> Result<Arc<ConnectedClient>, BridgeError> {
        let mut connected = self.inner.connected.lock().expect("desktop client lock");
        if let Some(client) = connected.as_ref() {
            return Ok(Arc::clone(client));
        }
        match ConnectedClient::connect(&self.inner.database) {
            Ok(client) => {
                let client = Arc::new(client);
                *connected = Some(Arc::clone(&client));
                return Ok(client);
            }
            Err(ClientError::IncompatibleProtocol) => return Err(BridgeError::incompatible()),
            Err(ClientError::InvalidEndpoint(_) | ClientError::Json(_)) => {
                return Err(BridgeError::disconnected());
            }
            Err(_) => {}
        }
        self.inner.starter.start(&self.inner.database)?;
        for _ in 0..CONNECT_ATTEMPTS {
            thread::sleep(CONNECT_DELAY);
            match ConnectedClient::connect(&self.inner.database) {
                Ok(client) => {
                    let client = Arc::new(client);
                    *connected = Some(Arc::clone(&client));
                    return Ok(client);
                }
                Err(ClientError::IncompatibleProtocol) => return Err(BridgeError::incompatible()),
                Err(_) => {}
            }
        }
        Err(BridgeError::disconnected())
    }

    pub fn invalidate(&self, client: &Arc<ConnectedClient>) {
        let mut connected = self.inner.connected.lock().expect("desktop client lock");
        if connected
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, client))
        {
            *connected = None;
        }
    }
}
