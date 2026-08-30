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
pub use server::{ApplicationCommandHandler, CommandHandler, DaemonServer};
pub use watcher::WatchConfig;

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::Duration;

    use rusqlite::params;
    use serde_json::json;
    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_application::workspace::WorkboardApplication;
    use workboard_client::{EndpointDescriptor as ClientEndpoint, SubscriptionUpdate};
    use workboard_client_protocol::{
        CURRENT_PROTOCOL_VERSION, CommandCode, EntityRef, EventCursor, EventEnvelope, EventId,
        EventKind, EventPayload, InvalidationScope, ReadQueryCode, RequestId, ResyncReason,
        WorkspaceId as ClientWorkspaceId,
    };
    use workboard_core::{RepositoryId, WorkspaceId};

    use super::{
        DaemonClient, DaemonError, DaemonServer, EndpointDescriptor, EndpointRegistration,
        PROTOCOL_VERSION, WatchConfig, WriteCommand, endpoint_path, read_descriptor,
    };

    fn loopback() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
    }

    fn application_fixture(directory: &TempDir) -> (WorkboardApplication, WorkspaceId) {
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
        (application, workspace_id)
    }

    fn typed_client(server: &DaemonServer, token: &str) -> workboard_client::WorkboardClient {
        workboard_client::WorkboardClient::connect(ClientEndpoint {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            address: server.address(),
            token: token.to_owned(),
        })
        .expect("connect typed client")
    }

    fn append_event(database: &Path, workspace_id: WorkspaceId, sequence: u64) -> EventEnvelope {
        let workspace = ClientWorkspaceId::from_uuid(*workspace_id.as_uuid());
        let event = EventEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            event_version: 1,
            workspace_id: workspace,
            sequence,
            event_id: EventId::generate(),
            occurred_at: OffsetDateTime::now_utc(),
            owner: EntityRef::Workspace(workspace),
            entity_revision: sequence,
            kind: EventKind::ProjectionChanged,
            payload: Some(EventPayload::ProjectionChanged {
                entity: EntityRef::Workspace(workspace),
            }),
            invalidation_scope: Some(InvalidationScope {
                queries: vec![ReadQueryCode::BoardSnapshot],
                owners: Vec::new(),
            }),
            operation_correlation_id: RequestId::generate(),
            partial_outcomes: Vec::new(),
        };
        let mut connection = rusqlite::Connection::open(database).expect("open event database");
        let transaction = connection.transaction().expect("event transaction");
        transaction
            .execute(
                "UPDATE workspace_projection_revisions SET revision = ?2 WHERE workspace_id = ?1",
                params![workspace_id.to_string(), sequence as i64],
            )
            .expect("update revision");
        transaction
            .execute(
                "INSERT INTO client_events (
                     workspace_id, sequence, event_id, occurred_at, event_json, idempotency_key
                 ) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                params![
                    workspace_id.to_string(),
                    sequence as i64,
                    event.event_id.to_string(),
                    "2026-08-30T00:00:00Z",
                    serde_json::to_string(&event).expect("event JSON"),
                ],
            )
            .expect("insert event");
        transaction.commit().expect("commit event");
        event
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
    fn typed_client_negotiates_reads_and_advertises_only_saved_views_as_available() {
        let directory = TempDir::new().expect("temporary directory");
        let (application, workspace_id) = application_fixture(&directory);
        let server = DaemonServer::start_application(application, loopback(), "opaque-token")
            .expect("start daemon");
        let client = typed_client(&server, "opaque-token");
        assert_eq!(
            client.handshake().negotiated_read_version,
            CURRENT_PROTOCOL_VERSION
        );
        assert_eq!(client.handshake().command_capabilities.len(), 10);
        assert_eq!(
            client
                .handshake()
                .command_capabilities
                .iter()
                .filter(|capability| capability.available)
                .map(|capability| capability.code)
                .collect::<Vec<_>>(),
            vec![CommandCode::SaveBoardView]
        );
        assert!(
            client
                .handshake()
                .command_capabilities
                .iter()
                .filter(|capability| capability.code != CommandCode::SaveBoardView)
                .all(|capability| !capability.available)
        );
        let snapshot = client
            .board_snapshot(ClientWorkspaceId::from_uuid(*workspace_id.as_uuid()))
            .expect("board snapshot");
        assert_eq!(snapshot.workspace.title, "Workspace");
        let serialised = serde_json::to_string(&snapshot).expect("snapshot JSON");
        assert!(!serialised.contains("opaque-token"));
        let rejected = workboard_client::WorkboardClient::connect(ClientEndpoint {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            address: server.address(),
            token: "wrong-token".to_owned(),
        })
        .expect_err("wrong token");
        assert_eq!(rejected.code(), "authentication_failed");
    }

    #[test]
    fn subscription_replays_ordered_events_and_resyncs_after_daemon_restart() {
        let directory = TempDir::new().expect("temporary directory");
        let database = directory.path().join("workboard.sqlite");
        let (application, workspace_id) = application_fixture(&directory);
        let server = DaemonServer::start_application(application, loopback(), "opaque-token")
            .expect("start daemon");
        let address = server.address();
        let client = typed_client(&server, "opaque-token");
        let client_workspace = ClientWorkspaceId::from_uuid(*workspace_id.as_uuid());
        let cursor = EventCursor {
            daemon_instance_id: client.handshake().daemon_instance_id,
            sequence: 0,
        };
        let mut subscription = client
            .subscribe(client_workspace, Some(cursor))
            .expect("subscribe");
        let first = append_event(&database, workspace_id, 1);
        let second = append_event(&database, workspace_id, 2);
        assert_eq!(
            subscription.next_update().expect("first event"),
            SubscriptionUpdate::Event(first)
        );
        assert_eq!(
            subscription.next_update().expect("second event"),
            SubscriptionUpdate::Event(second)
        );
        drop(server);
        let restarted = DaemonServer::start_application(
            WorkboardApplication::open(&database).expect("reopen application"),
            address,
            "opaque-token",
        )
        .expect("restart daemon");
        let SubscriptionUpdate::Resynced {
            requirement,
            snapshot,
        } = subscription.next_update().expect("restart resync")
        else {
            panic!("resync expected");
        };
        assert_eq!(requirement.reason, ResyncReason::DaemonRestarted);
        assert_eq!(requirement.authoritative_revision, 2);
        assert_eq!(snapshot.workspace.id, client_workspace);
        drop(restarted);
    }

    #[test]
    fn subscription_resyncs_expired_and_ahead_cursors_without_filling_gaps() {
        let directory = TempDir::new().expect("temporary directory");
        let database = directory.path().join("workboard.sqlite");
        let (application, workspace_id) = application_fixture(&directory);
        let server = DaemonServer::start_application(application, loopback(), "opaque-token")
            .expect("start daemon");
        let client = typed_client(&server, "opaque-token");
        let client_workspace = ClientWorkspaceId::from_uuid(*workspace_id.as_uuid());
        append_event(&database, workspace_id, 2);
        let mut expired = client
            .subscribe(
                client_workspace,
                Some(EventCursor {
                    daemon_instance_id: client.handshake().daemon_instance_id,
                    sequence: 0,
                }),
            )
            .expect("expired subscription");
        let SubscriptionUpdate::Resynced { requirement, .. } =
            expired.next_update().expect("expired resync")
        else {
            panic!("expired resync expected");
        };
        assert_eq!(requirement.reason, ResyncReason::CursorExpired);
        let mut ahead = client
            .subscribe(
                client_workspace,
                Some(EventCursor {
                    daemon_instance_id: client.handshake().daemon_instance_id,
                    sequence: 3,
                }),
            )
            .expect("ahead subscription");
        let SubscriptionUpdate::Resynced { requirement, .. } =
            ahead.next_update().expect("gap resync")
        else {
            panic!("gap resync expected");
        };
        assert_eq!(requirement.reason, ResyncReason::Gap);
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
