use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, Channel, InvokeBody};
use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{Runtime, WebviewWindow, WebviewWindowBuilder};
use workboard_client_protocol::CommandOperation;

use crate::configure;
use crate::connection::{ConnectionManager, DaemonStarter};
use crate::runtime::DesktopRuntime;
use crate::test_support::{FakeDaemon, FakeDaemonOptions, wait_until};
use crate::types::{
    BootstrapHandshake, BootstrapState, BridgeError, ExecuteRequest, SubscribeRequest,
    SubscriptionMessage,
};

struct NoopStarter;

impl DaemonStarter for NoopStarter {
    fn start(&self, _: &Path) -> Result<(), BridgeError> {
        Ok(())
    }
}

struct FakeStarter {
    database: PathBuf,
    starts: AtomicUsize,
    daemon: Mutex<Option<FakeDaemon>>,
}

impl FakeStarter {
    fn new(database: PathBuf) -> Self {
        Self {
            database,
            starts: AtomicUsize::new(0),
            daemon: Mutex::new(None),
        }
    }
}

impl DaemonStarter for FakeStarter {
    fn start(&self, database: &Path) -> Result<(), BridgeError> {
        if database != self.database {
            return Err(BridgeError::disconnected());
        }
        self.starts.fetch_add(1, Ordering::Relaxed);
        *self.daemon.lock().expect("fake starter daemon") =
            Some(FakeDaemon::start(database, FakeDaemonOptions::default()));
        Ok(())
    }
}

fn runtime(database: &Path) -> DesktopRuntime {
    DesktopRuntime::new(ConnectionManager::new(
        database.to_path_buf(),
        Arc::new(NoopStarter),
    ))
}

fn channel() -> (
    Channel<SubscriptionMessage>,
    mpsc::Receiver<SubscriptionMessage>,
) {
    let (sender, receiver) = mpsc::channel();
    let channel = Channel::new(move |body| {
        sender
            .send(
                body.deserialize::<SubscriptionMessage>()
                    .expect("subscription message"),
            )
            .expect("subscription receiver");
        Ok(())
    });
    (channel, receiver)
}

fn closed_channel() -> Channel<SubscriptionMessage> {
    Channel::new(|_| Err(tauri::Error::FailedToReceiveMessage))
}

fn invoke_request(command: &str, body: Value) -> InvokeRequest {
    InvokeRequest {
        cmd: command.to_owned(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "http://tauri.localhost".parse().expect("Tauri URL"),
        body: InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_owned(),
    }
}

fn webview<R: Runtime>(app: &tauri::App<R>, label: &str) -> WebviewWindow<R> {
    WebviewWindowBuilder::new(app, label, Default::default())
        .build()
        .expect("mock webview")
}

#[test]
fn tauri_mock_runtime_allows_only_the_main_window_and_declared_commands() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let database = temporary.path().join("workboard.sqlite");
    let _daemon = FakeDaemon::start(&database, FakeDaemonOptions::default());
    let app = configure(mock_builder(), runtime(&database))
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let main = webview(&app, "main");
    let response = get_ipc_response(&main, invoke_request("workboard_handshake", Value::Null))
        .expect("main handshake")
        .deserialize::<BootstrapHandshake>()
        .expect("handshake response");
    assert_eq!(response.state, BootstrapState::ReadOnly);
    let secondary = webview(&app, "secondary");
    assert!(
        get_ipc_response(
            &secondary,
            invoke_request("workboard_handshake", Value::Null)
        )
        .is_err()
    );
    assert!(get_ipc_response(&main, invoke_request("open_socket", Value::Null)).is_err());
}

#[test]
fn invalid_oversized_and_unknown_requests_fail_before_daemon_forwarding() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let database = temporary.path().join("workboard.sqlite");
    let daemon = FakeDaemon::start(&database, FakeDaemonOptions::default());
    let runtime = runtime(&database);
    assert_eq!(runtime.handshake().state, BootstrapState::ReadOnly);
    let before = daemon.forwarded_queries.load(Ordering::Acquire);
    assert!(
        runtime
            .execute(ExecuteRequest {
                workspace_id: daemon.workspace_id,
                expected_revision: 0,
                idempotency_key: "x".repeat(70_000),
                command: CommandOperation::SaveBoardView,
            })
            .is_err()
    );
    let app = configure(mock_builder(), runtime)
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let main = webview(&app, "main");
    for body in [
        json!({
            "request": {
                "workspaceId": "not-a-uuid",
                "query": { "type": "workspace_summary" }
            }
        }),
        json!({
            "request": {
                "workspaceId": daemon.workspace_id,
                "query": { "type": "open_socket" }
            }
        }),
        json!({
            "request": {
                "workspaceId": daemon.workspace_id,
                "expectedRevision": -1,
                "idempotencyKey": "test",
                "command": { "type": "save_board_view" }
            }
        }),
    ] {
        assert!(get_ipc_response(&main, invoke_request("workboard_query", body)).is_err());
    }
    assert_eq!(daemon.forwarded_queries.load(Ordering::Acquire), before);
}

#[test]
fn channel_delivery_is_ordered_and_all_lifecycle_paths_cancel() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let database = temporary.path().join("workboard.sqlite");
    let daemon = FakeDaemon::start(&database, FakeDaemonOptions::default());
    let runtime = runtime(&database);
    let (event_channel, receiver) = channel();
    let receipt = runtime
        .subscribe(
            "main".to_owned(),
            SubscribeRequest::Start {
                workspace_id: daemon.workspace_id,
                cursor: None,
            },
            event_channel,
        )
        .expect("start subscription");
    assert!(matches!(
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("connected"),
        SubscriptionMessage::Connected { .. }
    ));
    daemon.push_event(1, None);
    daemon.push_event(2, None);
    assert!(matches!(
        receiver.recv_timeout(Duration::from_secs(2)).expect("event one"),
        SubscriptionMessage::Event(event) if event.sequence == 1
    ));
    assert!(matches!(
        receiver.recv_timeout(Duration::from_secs(2)).expect("event two"),
        SubscriptionMessage::Event(event) if event.sequence == 2
    ));
    runtime
        .subscribe(
            "main".to_owned(),
            SubscribeRequest::Cancel {
                subscription_id: receipt.subscription_id,
            },
            closed_channel(),
        )
        .expect("cancel subscription");
    wait_until(Duration::from_secs(2), || {
        daemon.active_subscriptions.load(Ordering::Acquire) == 0
    });

    let (first_channel, first_receiver) = channel();
    runtime
        .subscribe(
            "main".to_owned(),
            SubscribeRequest::Start {
                workspace_id: daemon.workspace_id,
                cursor: None,
            },
            first_channel,
        )
        .expect("first replacement");
    first_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("first connected");
    let (second_channel, second_receiver) = channel();
    runtime
        .subscribe(
            "main".to_owned(),
            SubscribeRequest::Start {
                workspace_id: daemon.workspace_id,
                cursor: None,
            },
            second_channel,
        )
        .expect("second replacement");
    second_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("second connected");
    assert_eq!(runtime.active_subscription_count(), 1);
    runtime.cancel_window("main");
    wait_until(Duration::from_secs(2), || {
        runtime.active_subscription_count() == 0
            && daemon.active_subscriptions.load(Ordering::Acquire) == 0
    });
    assert_eq!(daemon.accepted_subscriptions.load(Ordering::Acquire), 3);

    runtime
        .subscribe(
            "main".to_owned(),
            SubscribeRequest::Start {
                workspace_id: daemon.workspace_id,
                cursor: None,
            },
            closed_channel(),
        )
        .expect("closed channel subscription");
    wait_until(Duration::from_secs(2), || {
        runtime.active_subscription_count() == 0
            && daemon.active_subscriptions.load(Ordering::Acquire) == 0
            && daemon.accepted_subscriptions.load(Ordering::Acquire) == 4
    });
    assert_eq!(daemon.accepted_subscriptions.load(Ordering::Acquire), 4);
}

#[test]
fn daemon_restart_resyncs_without_duplicate_subscriptions() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let database = temporary.path().join("workboard.sqlite");
    let mut first = FakeDaemon::start(&database, FakeDaemonOptions::default());
    let runtime = runtime(&database);
    let (event_channel, receiver) = channel();
    runtime
        .subscribe(
            "main".to_owned(),
            SubscribeRequest::Start {
                workspace_id: first.workspace_id,
                cursor: None,
            },
            event_channel,
        )
        .expect("start subscription");
    receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("first connected");
    let workspace_id = first.workspace_id;
    first.stop();
    let second = FakeDaemon::start(
        &database,
        FakeDaemonOptions {
            workspace_id: Some(workspace_id),
            ..FakeDaemonOptions::default()
        },
    );
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut resyncing = false;
    let mut resynced = false;
    let mut received = Vec::new();
    while Instant::now() < deadline && !resynced {
        if let Ok(message) = receiver.recv_timeout(Duration::from_millis(250)) {
            received.push(message.clone());
            match message {
                SubscriptionMessage::Resyncing(_) => resyncing = true,
                SubscriptionMessage::Resynced { .. } => resynced = true,
                _ => {}
            }
        }
    }
    assert!(
        resyncing,
        "messages={received:?}, registry={}, accepted={}, queries={}",
        runtime.active_subscription_count(),
        second.accepted_subscriptions.load(Ordering::Acquire),
        second.forwarded_queries.load(Ordering::Acquire)
    );
    assert!(resynced, "{received:?}");
    second.push_event(1, None);
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut event_count = 0;
    while Instant::now() < deadline && event_count == 0 {
        if matches!(
            receiver.recv_timeout(Duration::from_millis(250)),
            Ok(SubscriptionMessage::Event(event)) if event.sequence == 1
        ) {
            event_count += 1;
        }
    }
    assert_eq!(event_count, 1);
    assert_eq!(second.max_active_subscriptions.load(Ordering::Acquire), 1);
    assert_eq!(second.accepted_subscriptions.load(Ordering::Acquire), 2);
    assert_eq!(runtime.active_subscription_count(), 1);
    runtime.cancel_window("main");
}

#[test]
fn endpoint_credentials_are_absent_from_responses_channels_and_errors() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let database = temporary.path().join("workboard.sqlite");
    let unsafe_daemon = FakeDaemon::start(
        &database,
        FakeDaemonOptions {
            expose_token_in_handshake: true,
            ..FakeDaemonOptions::default()
        },
    );
    let handshake = runtime(&database).handshake();
    let serialized = serde_json::to_string(&handshake).expect("serialize handshake");
    assert_eq!(handshake.state, BootstrapState::Incompatible);
    assert!(!serialized.contains(&unsafe_daemon.token));
    drop(unsafe_daemon);

    let daemon = FakeDaemon::start(&database, FakeDaemonOptions::default());
    let runtime = runtime(&database);
    let (event_channel, receiver) = channel();
    runtime
        .subscribe(
            "main".to_owned(),
            SubscribeRequest::Start {
                workspace_id: daemon.workspace_id,
                cursor: None,
            },
            event_channel,
        )
        .expect("start subscription");
    receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("connected");
    daemon.push_event(1, Some(daemon.token.clone()));
    let message = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("redacted channel failure");
    let serialized = serde_json::to_string(&message).expect("serialize message");
    assert!(matches!(message, SubscriptionMessage::Disconnected { .. }));
    assert!(!serialized.contains(&daemon.token));
}

#[test]
fn endpoint_discovery_starts_a_missing_daemon_once() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let database = temporary.path().join("workboard.sqlite");
    let starter = Arc::new(FakeStarter::new(database.clone()));
    let runtime = DesktopRuntime::new(ConnectionManager::new(database, starter.clone()));
    assert_eq!(runtime.handshake().state, BootstrapState::ReadOnly);
    assert_eq!(runtime.handshake().state, BootstrapState::ReadOnly);
    assert_eq!(starter.starts.load(Ordering::Acquire), 1);
}

#[cfg(windows)]
#[test]
fn windows_fake_daemon_smoke_completes_through_the_async_tauri_command() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let database = temporary.path().join("workboard.sqlite");
    let _daemon = FakeDaemon::start(
        &database,
        FakeDaemonOptions {
            ready: true,
            ..FakeDaemonOptions::default()
        },
    );
    let app = configure(mock_builder(), runtime(&database))
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let main = webview(&app, "main");
    let response = get_ipc_response(&main, invoke_request("workboard_handshake", Value::Null))
        .expect("Windows fake daemon handshake")
        .deserialize::<BootstrapHandshake>()
        .expect("Windows handshake response");
    assert_eq!(response.state, BootstrapState::Ready);
}
