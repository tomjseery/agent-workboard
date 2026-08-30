use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tauri::ipc::Channel;
use workboard_client::{ClientError, SubscriptionCancellation, SubscriptionUpdate};
use workboard_client_protocol::{EventCursor, WorkspaceId};

use crate::connection::{ConnectedClient, ConnectionManager};
use crate::types::{BridgeError, SubscriptionMessage};

const RETRY_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SubscriptionScope {
    window_label: String,
    workspace_id: WorkspaceId,
}

struct SubscriptionControl {
    cancelled: AtomicBool,
    cancellation: Mutex<Option<SubscriptionCancellation>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl SubscriptionControl {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            cancellation: Mutex::new(None),
            thread: Mutex::new(None),
        }
    }

    fn attach(&self, cancellation: SubscriptionCancellation) {
        if self.cancelled.load(Ordering::Acquire) {
            cancellation.cancel();
        } else {
            *self.cancellation.lock().expect("subscription cancel lock") = Some(cancellation);
        }
    }

    fn clear(&self) {
        self.cancellation
            .lock()
            .expect("subscription cancel lock")
            .take();
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(cancellation) = self
            .cancellation
            .lock()
            .expect("subscription cancel lock")
            .take()
        {
            cancellation.cancel();
        }
    }

    fn cancel_and_join(&self) {
        self.cancel();
        if let Some(thread) = self.thread.lock().expect("subscription thread lock").take() {
            let _ = thread.join();
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn wait_retry(&self) {
        for _ in 0..10 {
            if self.is_cancelled() {
                return;
            }
            thread::sleep(RETRY_DELAY / 10);
        }
    }
}

struct RegistryEntry {
    scope: SubscriptionScope,
    control: Arc<SubscriptionControl>,
}

#[derive(Default)]
struct Registry {
    entries: HashMap<u64, RegistryEntry>,
    scopes: HashMap<SubscriptionScope, u64>,
}

#[derive(Clone)]
pub(crate) struct SubscriptionManager {
    inner: Arc<SubscriptionManagerInner>,
}

struct SubscriptionManagerInner {
    connections: ConnectionManager,
    next_id: AtomicU64,
    registry: Mutex<Registry>,
}

impl SubscriptionManager {
    pub fn new(connections: ConnectionManager) -> Self {
        Self {
            inner: Arc::new(SubscriptionManagerInner {
                connections,
                next_id: AtomicU64::new(1),
                registry: Mutex::new(Registry::default()),
            }),
        }
    }

    pub fn start(
        &self,
        window_label: String,
        workspace_id: WorkspaceId,
        cursor: Option<EventCursor>,
        channel: Channel<SubscriptionMessage>,
    ) -> u64 {
        let subscription_id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let scope = SubscriptionScope {
            window_label,
            workspace_id,
        };
        let control = Arc::new(SubscriptionControl::new());
        let previous = {
            let mut registry = self.inner.registry.lock().expect("subscription registry");
            let previous = registry
                .scopes
                .insert(scope.clone(), subscription_id)
                .and_then(|id| registry.entries.remove(&id));
            registry.entries.insert(
                subscription_id,
                RegistryEntry {
                    scope,
                    control: Arc::clone(&control),
                },
            );
            previous
        };
        if let Some(previous) = previous {
            previous.control.cancel_and_join();
        }
        let manager = self.clone();
        let worker_control = Arc::clone(&control);
        let worker = thread::spawn(move || {
            manager.run_worker(
                subscription_id,
                workspace_id,
                cursor,
                channel,
                worker_control,
            );
        });
        *control.thread.lock().expect("subscription thread lock") = Some(worker);
        subscription_id
    }

    pub fn cancel(&self, window_label: &str, subscription_id: u64) -> Result<(), BridgeError> {
        if subscription_id == 0 {
            return Err(BridgeError::invalid_request());
        }
        let entry = {
            let mut registry = self.inner.registry.lock().expect("subscription registry");
            let Some(entry) = registry.entries.get(&subscription_id) else {
                return Err(BridgeError::invalid_request());
            };
            if entry.scope.window_label != window_label {
                return Err(BridgeError::forbidden_window());
            }
            let entry = registry
                .entries
                .remove(&subscription_id)
                .expect("registered subscription");
            registry.scopes.remove(&entry.scope);
            entry
        };
        entry.control.cancel_and_join();
        Ok(())
    }

    pub fn cancel_window(&self, window_label: &str) {
        let entries = {
            let mut registry = self.inner.registry.lock().expect("subscription registry");
            let ids = registry
                .entries
                .iter()
                .filter_map(|(id, entry)| (entry.scope.window_label == window_label).then_some(*id))
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| {
                    let entry = registry.entries.remove(&id)?;
                    registry.scopes.remove(&entry.scope);
                    Some(entry)
                })
                .collect::<Vec<_>>()
        };
        for entry in entries {
            entry.control.cancel();
        }
    }

    #[cfg(test)]
    pub fn active_count(&self) -> usize {
        self.inner
            .registry
            .lock()
            .expect("subscription registry")
            .entries
            .len()
    }

    fn finish(&self, subscription_id: u64) {
        let mut registry = self.inner.registry.lock().expect("subscription registry");
        if let Some(entry) = registry.entries.remove(&subscription_id) {
            registry.scopes.remove(&entry.scope);
        }
    }

    fn run_worker(
        &self,
        subscription_id: u64,
        workspace_id: WorkspaceId,
        mut cursor: Option<EventCursor>,
        channel: Channel<SubscriptionMessage>,
        control: Arc<SubscriptionControl>,
    ) {
        while !control.is_cancelled() {
            let client = match self.inner.connections.client() {
                Ok(client) => client,
                Err(error) if error.code == "incompatible_protocol" => {
                    let _ = channel.send(SubscriptionMessage::Incompatible);
                    break;
                }
                Err(error) => {
                    if !send(
                        &channel,
                        SubscriptionMessage::Disconnected { code: error.code },
                        &control,
                    ) {
                        break;
                    }
                    control.wait_retry();
                    continue;
                }
            };
            let mut subscription = match client.client.subscribe(workspace_id, cursor) {
                Ok(subscription) => subscription,
                Err(error) => {
                    self.inner.connections.invalidate(&client);
                    if is_incompatible(&error) {
                        let _ = channel.send(SubscriptionMessage::Incompatible);
                        break;
                    }
                    if !send(
                        &channel,
                        SubscriptionMessage::Disconnected {
                            code: "daemon_unavailable".to_owned(),
                        },
                        &control,
                    ) {
                        break;
                    }
                    control.wait_retry();
                    continue;
                }
            };
            cursor = Some(subscription.cursor());
            control.attach(subscription.cancellation_handle());
            if !send_guarded(
                &channel,
                SubscriptionMessage::Connected {
                    state: client.state(),
                },
                &control,
                &client,
            ) {
                break;
            }
            loop {
                match subscription.next_update_without_reconnect() {
                    Ok(SubscriptionUpdate::Event(event)) => {
                        cursor = Some(subscription.cursor());
                        if !send_guarded(
                            &channel,
                            SubscriptionMessage::Event(event),
                            &control,
                            &client,
                        ) {
                            control.cancel();
                            break;
                        }
                    }
                    Ok(SubscriptionUpdate::Resynced {
                        requirement,
                        snapshot,
                    }) => {
                        cursor = Some(subscription.cursor());
                        if !send_guarded(
                            &channel,
                            SubscriptionMessage::Resyncing(requirement.clone()),
                            &control,
                            &client,
                        ) || !send_guarded(
                            &channel,
                            SubscriptionMessage::Resynced {
                                requirement,
                                snapshot,
                            },
                            &control,
                            &client,
                        ) || !send_guarded(
                            &channel,
                            SubscriptionMessage::Connected {
                                state: client.state(),
                            },
                            &control,
                            &client,
                        ) {
                            control.cancel();
                            break;
                        }
                    }
                    Err(ClientError::SubscriptionCancelled) => break,
                    Err(error) => {
                        self.inner.connections.invalidate(&client);
                        let message = if is_incompatible(&error) {
                            SubscriptionMessage::Incompatible
                        } else {
                            SubscriptionMessage::Disconnected {
                                code: "daemon_unavailable".to_owned(),
                            }
                        };
                        let _ = send(&channel, message, &control);
                        break;
                    }
                }
            }
            control.clear();
            control.wait_retry();
        }
        self.finish(subscription_id);
    }
}

fn is_incompatible(error: &ClientError) -> bool {
    matches!(error, ClientError::IncompatibleProtocol)
        || matches!(error, ClientError::Remote(remote) if remote.code == "incompatible_protocol")
}

fn send(
    channel: &Channel<SubscriptionMessage>,
    message: SubscriptionMessage,
    control: &SubscriptionControl,
) -> bool {
    if control.is_cancelled() {
        return false;
    }
    if channel.send(message).is_err() {
        control.cancel();
        return false;
    }
    true
}

fn send_guarded(
    channel: &Channel<SubscriptionMessage>,
    message: SubscriptionMessage,
    control: &SubscriptionControl,
    client: &Arc<ConnectedClient>,
) -> bool {
    if client.ensure_safe(&message).is_err() {
        let _ = send(
            channel,
            SubscriptionMessage::Disconnected {
                code: "unsafe_daemon_payload".to_owned(),
            },
            control,
        );
        return false;
    }
    send(channel, message, control)
}
