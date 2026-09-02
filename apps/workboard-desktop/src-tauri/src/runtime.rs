use std::sync::Arc;

use tauri::ipc::Channel;
use workboard_client::ClientError;
use workboard_client_protocol::{ResponseEnvelope, WorkspaceId};

use crate::connection::ConnectionManager;
use crate::subscriptions::SubscriptionManager;
use crate::types::{
    BootstrapHandshake, BootstrapState, BridgeError, ExecuteRequest, QueryRequest,
    SubscribeRequest, SubscriptionMessage, SubscriptionReceipt, SubscriptionTarget,
    validate_request,
};

#[derive(Clone)]
pub struct DesktopRuntime {
    connections: ConnectionManager,
    subscriptions: SubscriptionManager,
}

impl DesktopRuntime {
    pub fn system() -> Result<Self, BridgeError> {
        Ok(Self::new(ConnectionManager::system()?))
    }

    pub(crate) fn new(connections: ConnectionManager) -> Self {
        Self {
            subscriptions: SubscriptionManager::new(connections.clone()),
            connections,
        }
    }

    pub fn handshake(&self) -> BootstrapHandshake {
        match self.connections.client() {
            Ok(client) if client.ensure_safe(client.client.handshake()).is_ok() => {
                BootstrapHandshake {
                    state: client.state(),
                    subscriptions: client
                        .client
                        .handshake()
                        .workspaces
                        .iter()
                        .map(|workspace| SubscriptionTarget {
                            workspace_id: workspace.id,
                        })
                        .collect(),
                    refusal: None,
                }
            }
            Ok(_) => BootstrapHandshake {
                state: BootstrapState::Incompatible,
                subscriptions: Vec::new(),
                refusal: None,
            },
            Err(error) if error.code == "incompatible_protocol" => BootstrapHandshake {
                state: BootstrapState::Incompatible,
                subscriptions: Vec::new(),
                refusal: Some(error.message),
            },
            Err(error) => BootstrapHandshake {
                state: BootstrapState::Disconnected,
                subscriptions: Vec::new(),
                refusal: Some(error.message),
            },
        }
    }

    pub fn query(&self, request: QueryRequest) -> Result<ResponseEnvelope, BridgeError> {
        validate_request(&request)?;
        validate_workspace_id(request.workspace_id)?;
        let client = self.connections.client()?;
        let response = client
            .client
            .query_reported(request.workspace_id, request.query)
            .map_err(|error| self.map_client_error(&client, error))?;
        client.ensure_safe(&response)?;
        Ok(response)
    }

    pub fn execute(&self, request: ExecuteRequest) -> Result<ResponseEnvelope, BridgeError> {
        validate_request(&request)?;
        validate_workspace_id(request.workspace_id)?;
        if request.idempotency_key.is_empty()
            || request.idempotency_key.len() > 200
            || request.idempotency_key.chars().any(char::is_control)
        {
            return Err(BridgeError::invalid_request());
        }
        let client = self.connections.client()?;
        let response = client
            .client
            .execute(
                request.workspace_id,
                request.expected_revision,
                request.idempotency_key,
                request.command,
            )
            .map_err(|error| self.map_client_error(&client, error))?;
        client.ensure_safe(&response)?;
        Ok(response)
    }

    pub fn subscribe(
        &self,
        window_label: String,
        request: SubscribeRequest,
        channel: Channel<SubscriptionMessage>,
    ) -> Result<SubscriptionReceipt, BridgeError> {
        validate_request(&request)?;
        match request {
            SubscribeRequest::Start {
                workspace_id,
                cursor,
            } => {
                validate_workspace_id(workspace_id)?;
                let subscription_id =
                    self.subscriptions
                        .start(window_label, workspace_id, cursor, channel);
                Ok(SubscriptionReceipt { subscription_id })
            }
            SubscribeRequest::Cancel { subscription_id } => {
                drop(channel);
                self.subscriptions.cancel(&window_label, subscription_id)?;
                Ok(SubscriptionReceipt { subscription_id })
            }
        }
    }

    pub fn cancel_window(&self, window_label: &str) {
        self.subscriptions.cancel_window(window_label);
    }

    #[cfg(test)]
    pub fn active_subscription_count(&self) -> usize {
        self.subscriptions.active_count()
    }

    fn map_client_error(
        &self,
        client: &Arc<crate::connection::ConnectedClient>,
        error: ClientError,
    ) -> BridgeError {
        self.connections.invalidate(client);
        if matches!(error, ClientError::IncompatibleProtocol)
            || matches!(&error, ClientError::Remote(remote) if remote.code == "incompatible_protocol")
        {
            BridgeError::incompatible()
        } else {
            BridgeError::disconnected()
        }
    }
}

pub fn validate_workspace_id(workspace_id: WorkspaceId) -> Result<(), BridgeError> {
    if workspace_id.as_uuid().is_nil() {
        return Err(BridgeError::invalid_request());
    }
    Ok(())
}
