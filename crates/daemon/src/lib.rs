#![forbid(unsafe_code)]

mod client;
mod endpoint;
mod error;
mod protocol;
mod server;
mod watcher;

pub use client::DaemonClient;
pub use endpoint::{EndpointDescriptor, EndpointRegistration, endpoint_path, read_descriptor};
pub use error::DaemonError;
pub use protocol::{PROTOCOL_VERSION, RemoteError, WriteCommand};
pub use server::{CommandHandler, DaemonServer};
pub use watcher::WatchConfig;

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::Duration;

    use serde_json::json;
    use tempfile::TempDir;

    use super::{
        DaemonClient, DaemonError, DaemonServer, EndpointDescriptor, EndpointRegistration,
        PROTOCOL_VERSION, WatchConfig, WriteCommand, endpoint_path, read_descriptor,
    };

    fn loopback() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
    }

    #[test]
    fn authenticates_requests_and_serialises_writer_access() {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let handled = Arc::new(AtomicUsize::new(0));
        let handler_active = Arc::clone(&active);
        let handler_maximum = Arc::clone(&maximum);
        let handler_handled = Arc::clone(&handled);
        let server = DaemonServer::start(
            move |_| {
                let current = handler_active.fetch_add(1, Ordering::SeqCst) + 1;
                handler_maximum.fetch_max(current, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(5));
                handler_active.fetch_sub(1, Ordering::SeqCst);
                let ordinal = handler_handled.fetch_add(1, Ordering::SeqCst) + 1;
                Ok(json!({ "ordinal": ordinal }))
            },
            loopback(),
            "opaque-token",
        )
        .expect("start daemon");

        assert_eq!(
            server.client().request(WriteCommand::Ping).expect("ping"),
            json!({ "status": "ready" })
        );
        let rejected = DaemonClient::new(server.address(), "wrong-token")
            .request(WriteCommand::Ping)
            .expect_err("wrong token should fail");
        assert_eq!(rejected.code(), "authentication_failed");

        let clients = (0..8)
            .map(|_| {
                let client = server.client();
                thread::spawn(move || {
                    client.request(WriteCommand::RefreshNativeSessions {
                        claude_root: None,
                        codex_root: None,
                    })
                })
            })
            .collect::<Vec<_>>();
        for client in clients {
            client
                .join()
                .expect("client thread")
                .expect("refresh request");
        }

        assert_eq!(handled.load(Ordering::SeqCst), 8);
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn watcher_uses_the_serial_writer_for_initial_refresh() {
        let directory = TempDir::new().expect("temporary directory");
        let (sender, receiver) = mpsc::channel();
        let mut server = DaemonServer::start(
            move |command| {
                sender.send(command).expect("record command");
                Ok(json!({ "status": "refreshed" }))
            },
            loopback(),
            "opaque-token",
        )
        .expect("start daemon");
        server
            .enable_watcher(WatchConfig {
                claude_root: Some(directory.path().to_path_buf()),
                codex_root: None,
                debounce: Duration::from_millis(10),
                reconcile_interval: Duration::from_secs(60),
            })
            .expect("enable watcher");

        let observed = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("initial refresh");
        assert!(matches!(
            observed,
            WriteCommand::RefreshNativeSessions {
                claude_root: Some(_),
                codex_root: None
            }
        ));
    }

    #[test]
    fn endpoint_claim_is_exclusive_and_descriptor_is_recoverable() {
        let directory = TempDir::new().expect("temporary directory");
        let database = directory.path().join("workboard.sqlite");
        std::fs::write(&database, []).expect("database fixture");
        let descriptor = EndpointDescriptor {
            protocol_version: PROTOCOL_VERSION,
            address: loopback(),
            token: "opaque-token".to_owned(),
        };
        let registration = EndpointRegistration::claim(&database, &descriptor).expect("claim");
        let path = endpoint_path(&database);

        assert_eq!(read_descriptor(&path).expect("descriptor"), descriptor);
        assert!(matches!(
            EndpointRegistration::acquire(&database),
            Err(DaemonError::AlreadyRunning)
        ));
        drop(registration);
        assert!(!path.exists());
        EndpointRegistration::acquire(&database).expect("reclaim after release");
    }

    #[test]
    fn rejects_unsafe_listener_identity() {
        assert!(matches!(
            DaemonServer::start(
                |_| Ok(json!(null)),
                "0.0.0.0:0".parse().expect("address"),
                "token"
            ),
            Err(DaemonError::NonLoopbackAddress(_))
        ));
        assert!(matches!(
            DaemonServer::start(|_| Ok(json!(null)), loopback(), ""),
            Err(DaemonError::InvalidToken)
        ));
    }
}
