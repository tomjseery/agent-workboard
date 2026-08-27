use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::DaemonError;

pub(crate) const ENDPOINT_LOCK_MARKER: &[u8] = b"agent-workboard/endpoint-lock/os-v1\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointDescriptor {
    pub protocol_version: u32,
    pub address: SocketAddr,
    pub token: String,
}

pub struct EndpointRegistration {
    endpoint_path: PathBuf,
    lock: Option<File>,
}

impl EndpointRegistration {
    pub fn claim(database: &Path, descriptor: &EndpointDescriptor) -> Result<Self, DaemonError> {
        let registration = Self::acquire(database)?;
        registration.publish(descriptor)?;
        Ok(registration)
    }

    pub fn acquire(database: &Path) -> Result<Self, DaemonError> {
        let endpoint_path = endpoint_path(database);
        let lock_path = endpoint_path.with_extension("lock");
        let (mut lock, existing_lock) = open_lock(&lock_path)?;
        for attempt in 0..5 {
            match lock.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) if !endpoint_path.exists() && attempt < 4 => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(TryLockError::WouldBlock) => {
                    return Err(DaemonError::AlreadyRunning);
                }
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
        }
        if existing_lock {
            if !has_lock_marker(&mut lock)? {
                return Err(DaemonError::LegacyOwnership);
            }
        } else if let Err(error) = initialise_lock(&mut lock) {
            drop(lock);
            let _ = fs::remove_file(&lock_path);
            return Err(error);
        }
        Ok(Self {
            endpoint_path,
            lock: Some(lock),
        })
    }

    pub fn publish(&self, descriptor: &EndpointDescriptor) -> Result<(), DaemonError> {
        write_descriptor(&self.endpoint_path, descriptor)
    }
}

impl Drop for EndpointRegistration {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.endpoint_path);
        self.lock.take();
    }
}

fn open_lock(path: &Path) -> Result<(File, bool), DaemonError> {
    for _ in 0..3 {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(lock) => return Ok((lock, false)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                match OpenOptions::new().read(true).write(true).open(path) {
                    Ok(lock) => return Ok((lock, true)),
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(DaemonError::AlreadyRunning)
}

fn initialise_lock(lock: &mut File) -> Result<(), DaemonError> {
    lock.set_len(0)?;
    lock.seek(SeekFrom::Start(0))?;
    lock.write_all(ENDPOINT_LOCK_MARKER)?;
    lock.sync_data()?;
    Ok(())
}

fn has_lock_marker(lock: &mut File) -> Result<bool, DaemonError> {
    lock.seek(SeekFrom::Start(0))?;
    let mut contents = Vec::new();
    lock.read_to_end(&mut contents)?;
    Ok(contents == ENDPOINT_LOCK_MARKER)
}

pub fn endpoint_path(database: &Path) -> PathBuf {
    database.with_extension("workboardd.json")
}

pub fn read_descriptor(path: &Path) -> Result<EndpointDescriptor, DaemonError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn write_descriptor(path: &Path, descriptor: &EndpointDescriptor) -> Result<(), DaemonError> {
    let temporary = path.with_extension("workboardd.json.tmp");
    fs::write(&temporary, serde_json::to_vec(descriptor)?)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}
