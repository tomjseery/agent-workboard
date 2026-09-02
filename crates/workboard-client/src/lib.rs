#![forbid(unsafe_code)]

mod error;
pub mod framing;

use std::fs;
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub use error::ClientError;
use serde::{Deserialize, Serialize};
use workboard_client_protocol::{
    BoardSnapshot, CURRENT_PROTOCOL_VERSION, CommandOperation, EventCursor, EventEnvelope,
    HandshakeRequest, HandshakeResponse, HierarchyChildren, HierarchyRef, Operation,
    PREVIOUS_PROTOCOL_VERSION, ProtocolError, ReadQuery, ReadQueryCode, RequestEnvelope, RequestId,
    ResponseEnvelope, ResponseResult, ResyncReason, ResyncRequirement, SUPPORTED_READ_VERSIONS,
    ServerMessage, SubscriptionRequest, WorkspaceId, WorkspaceSummary,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const RECONNECT_DELAY: Duration = Duration::from_millis(50);
const MIN_SUBSCRIPTION_TIMEOUT_MS: u64 = 100;
const MAX_SUBSCRIPTION_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointDescriptor {
    pub protocol_version: u32,
    pub address: SocketAddr,
    pub token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthenticatedRequest<'a> {
    token: &'a str,
    request: &'a RequestEnvelope,
}

#[derive(Debug, Clone)]
pub struct WorkboardClient {
    endpoint: EndpointDescriptor,
    timeout: Duration,
    handshake: HandshakeResponse,
}

impl WorkboardClient {
    pub fn discover(database: &Path) -> Result<Self, ClientError> {
        let descriptor = read_descriptor(&endpoint_path(database))?;
        Self::connect(descriptor)
    }

    pub fn connect(endpoint: EndpointDescriptor) -> Result<Self, ClientError> {
        validate_endpoint(&endpoint)?;
        let placeholder = HandshakeResponse {
            daemon_instance_id: workboard_client_protocol::DaemonInstanceId::generate(),
            negotiated_read_version: CURRENT_PROTOCOL_VERSION,
            compatible_command_versions: Vec::new(),
            workspaces: Vec::new(),
            command_capabilities: Vec::new(),
            event_version: 1,
            heartbeat_interval_ms: 1_000,
            max_frame_bytes: workboard_client_protocol::MAX_FRAME_BYTES,
        };
        let mut client = Self {
            endpoint,
            timeout: DEFAULT_TIMEOUT,
            handshake: placeholder,
        };
        client.handshake = client.perform_handshake()?;
        Ok(client)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn handshake(&self) -> &HandshakeResponse {
        &self.handshake
    }

    pub fn sole_workspace_id(&self) -> Result<WorkspaceId, ClientError> {
        match self.handshake.workspaces.as_slice() {
            [workspace] => Ok(workspace.id),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    pub fn workspace_summary(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceSummary, ClientError> {
        match self
            .query(workspace_id, ReadQuery::WorkspaceSummary)?
            .result
        {
            Some(ResponseResult::WorkspaceSummary(summary)) => Ok(summary),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    pub fn hierarchy_children(
        &self,
        workspace_id: WorkspaceId,
        parent: HierarchyRef,
    ) -> Result<HierarchyChildren, ClientError> {
        match self
            .query(workspace_id, ReadQuery::HierarchyChildren { parent })?
            .result
        {
            Some(ResponseResult::HierarchyChildren(children)) => Ok(children),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    pub fn board_snapshot(&self, workspace_id: WorkspaceId) -> Result<BoardSnapshot, ClientError> {
        match self.query(workspace_id, ReadQuery::BoardSnapshot)?.result {
            Some(ResponseResult::BoardSnapshot(snapshot)) => Ok(snapshot),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    pub fn subscribe(
        &self,
        workspace_id: WorkspaceId,
        cursor: Option<EventCursor>,
    ) -> Result<Subscription, ClientError> {
        Subscription::connect(self.clone(), workspace_id, cursor)
    }

    fn perform_handshake(&self) -> Result<HandshakeResponse, ClientError> {
        let request = RequestEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            request_id: RequestId::generate(),
            workspace_id: None,
            expected_revision: None,
            idempotency_key: None,
            operation: Operation::Handshake(HandshakeRequest {
                supported_read_versions: SUPPORTED_READ_VERSIONS.to_vec(),
                supported_command_versions: vec![CURRENT_PROTOCOL_VERSION],
            }),
        };
        match self.send(&request)?.result {
            Some(ResponseResult::Handshake(handshake)) => Ok(handshake),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    pub fn query(
        &self,
        workspace_id: WorkspaceId,
        query: ReadQuery,
    ) -> Result<ResponseEnvelope, ClientError> {
        let request = RequestEnvelope {
            protocol_version: self.handshake.negotiated_read_version,
            request_id: RequestId::generate(),
            workspace_id: Some(workspace_id),
            expected_revision: None,
            idempotency_key: None,
            operation: Operation::Query(query),
        };
        self.send(&request)
    }

    pub fn query_reported(
        &self,
        workspace_id: WorkspaceId,
        query: ReadQuery,
    ) -> Result<ResponseEnvelope, ClientError> {
        let request = RequestEnvelope {
            protocol_version: self.handshake.negotiated_read_version,
            request_id: RequestId::generate(),
            workspace_id: Some(workspace_id),
            expected_revision: None,
            idempotency_key: None,
            operation: Operation::Query(query),
        };
        self.send_reported(&request)
    }

    pub fn execute(
        &self,
        workspace_id: WorkspaceId,
        expected_revision: u64,
        idempotency_key: String,
        command: CommandOperation,
    ) -> Result<ResponseEnvelope, ClientError> {
        let request = RequestEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            request_id: RequestId::generate(),
            workspace_id: Some(workspace_id),
            expected_revision: Some(expected_revision),
            idempotency_key: Some(idempotency_key),
            operation: Operation::Command(command),
        };
        self.send(&request)
    }

    fn send(&self, request: &RequestEnvelope) -> Result<ResponseEnvelope, ClientError> {
        let response = self.send_reported(request)?;
        if let Some(error) = response.error.clone() {
            return Err(ClientError::Remote(Box::new(error)));
        }
        Ok(response)
    }

    fn send_reported(&self, request: &RequestEnvelope) -> Result<ResponseEnvelope, ClientError> {
        request.validate().map_err(ClientError::Remote)?;
        let mut stream = self.open_stream()?;
        framing::write_frame(
            &mut stream,
            &AuthenticatedRequest {
                token: &self.endpoint.token,
                request,
            },
        )?;
        match framing::read_frame::<ServerMessage>(&mut stream)? {
            ServerMessage::Response(response) => correlate_response(request, *response),
            _ => Err(ClientError::UnexpectedResponse),
        }
    }

    fn open_stream(&self) -> Result<TcpStream, ClientError> {
        let stream = TcpStream::connect_timeout(&self.endpoint.address, self.timeout)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        Ok(stream)
    }
}

fn correlate_response(
    request: &RequestEnvelope,
    response: ResponseEnvelope,
) -> Result<ResponseEnvelope, ClientError> {
    if response.request_id != request.request_id || response.correlation_id != request.request_id {
        return Err(ClientError::UnexpectedResponse);
    }
    if !SUPPORTED_READ_VERSIONS.contains(&response.protocol_version) {
        return Err(ClientError::IncompatibleProtocol);
    }
    Ok(response)
}

fn validate_response(
    request: &RequestEnvelope,
    response: ResponseEnvelope,
) -> Result<ResponseEnvelope, ClientError> {
    let response = correlate_response(request, response)?;
    if let Some(error) = response.error.clone() {
        return Err(ClientError::Remote(Box::new(error)));
    }
    Ok(response)
}

pub struct Subscription {
    client: WorkboardClient,
    workspace_id: WorkspaceId,
    cursor: EventCursor,
    stream: TcpStream,
    cancellation: Arc<SubscriptionCancellationState>,
}

impl Subscription {
    fn connect(
        client: WorkboardClient,
        workspace_id: WorkspaceId,
        cursor: Option<EventCursor>,
    ) -> Result<Self, ClientError> {
        Self::connect_with_cancellation(
            client,
            workspace_id,
            cursor,
            Arc::new(SubscriptionCancellationState::default()),
        )
    }

    fn connect_with_cancellation(
        client: WorkboardClient,
        workspace_id: WorkspaceId,
        cursor: Option<EventCursor>,
        cancellation: Arc<SubscriptionCancellationState>,
    ) -> Result<Self, ClientError> {
        if cancellation.cancelled.load(Ordering::Acquire) {
            return Err(ClientError::SubscriptionCancelled);
        }
        let mut stream = client.open_stream()?;
        let subscription_timeout = Duration::from_millis(
            client
                .handshake
                .heartbeat_interval_ms
                .saturating_mul(3)
                .clamp(MIN_SUBSCRIPTION_TIMEOUT_MS, MAX_SUBSCRIPTION_TIMEOUT_MS),
        );
        stream.set_read_timeout(Some(subscription_timeout))?;
        let request = RequestEnvelope {
            protocol_version: client.handshake.negotiated_read_version,
            request_id: RequestId::generate(),
            workspace_id: Some(workspace_id),
            expected_revision: None,
            idempotency_key: None,
            operation: Operation::Subscribe(SubscriptionRequest { cursor }),
        };
        framing::write_frame(
            &mut stream,
            &AuthenticatedRequest {
                token: &client.endpoint.token,
                request: &request,
            },
        )?;
        let ServerMessage::Response(response) = framing::read_frame(&mut stream)? else {
            return Err(ClientError::UnexpectedResponse);
        };
        let response = validate_response(&request, *response)?;
        let Some(ResponseResult::SubscriptionAccepted { cursor }) = response.result else {
            return Err(ClientError::UnexpectedResponse);
        };
        cancellation.set_stream(&stream)?;
        Ok(Self {
            client,
            workspace_id,
            cursor,
            stream,
            cancellation,
        })
    }

    pub fn cursor(&self) -> EventCursor {
        self.cursor
    }

    pub fn cancellation_handle(&self) -> SubscriptionCancellation {
        SubscriptionCancellation {
            state: Arc::clone(&self.cancellation),
        }
    }

    pub fn next_update(&mut self) -> Result<SubscriptionUpdate, ClientError> {
        self.next_update_with_reconnect(true)
    }

    pub fn next_update_without_reconnect(&mut self) -> Result<SubscriptionUpdate, ClientError> {
        self.next_update_with_reconnect(false)
    }

    fn next_update_with_reconnect(
        &mut self,
        reconnect: bool,
    ) -> Result<SubscriptionUpdate, ClientError> {
        loop {
            let result = framing::read_frame::<ServerMessage>(&mut self.stream);
            if self.cancellation.cancelled.load(Ordering::Acquire) {
                return Err(ClientError::SubscriptionCancelled);
            }
            match result {
                Ok(ServerMessage::Event(event)) => return self.accept_event(*event),
                Ok(ServerMessage::Heartbeat(_)) => {}
                Ok(ServerMessage::ResyncRequired(requirement)) => {
                    return self.resync(requirement);
                }
                Ok(ServerMessage::Response(_)) => return Err(ClientError::UnexpectedResponse),
                Err(ClientError::Io(error)) if reconnect && is_disconnect(&error) => {
                    self.reconnect()?
                }
                Err(ClientError::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    let requirement = ResyncRequirement {
                        reason: ResyncReason::HeartbeatLost,
                        workspace_id: self.workspace_id,
                        authoritative_revision: self.cursor.sequence,
                        oldest_replayable_sequence: self.cursor.sequence,
                        required_queries: vec![ReadQueryCode::BoardSnapshot],
                    };
                    return self.resync(requirement);
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn accept_event(&mut self, event: EventEnvelope) -> Result<SubscriptionUpdate, ClientError> {
        if event.workspace_id != self.workspace_id || event.event_version != 1 {
            let requirement = ResyncRequirement {
                reason: ResyncReason::IncompatibleEvent,
                workspace_id: self.workspace_id,
                authoritative_revision: event.entity_revision,
                oldest_replayable_sequence: self.cursor.sequence,
                required_queries: vec![ReadQueryCode::BoardSnapshot],
            };
            return self.resync(requirement);
        }
        if event.sequence != self.cursor.sequence + 1 {
            let requirement = ResyncRequirement {
                reason: ResyncReason::Gap,
                workspace_id: self.workspace_id,
                authoritative_revision: event.entity_revision,
                oldest_replayable_sequence: self.cursor.sequence,
                required_queries: vec![ReadQueryCode::BoardSnapshot],
            };
            return self.resync(requirement);
        }
        self.cursor.sequence = event.sequence;
        Ok(SubscriptionUpdate::Event(event))
    }

    fn reconnect(&mut self) -> Result<(), ClientError> {
        let mut last_error = None;
        for _ in 0..20 {
            if self.cancellation.cancelled.load(Ordering::Acquire) {
                return Err(ClientError::SubscriptionCancelled);
            }
            thread::sleep(RECONNECT_DELAY);
            match self.client.perform_handshake() {
                Ok(handshake) => {
                    self.client.handshake = handshake;
                    match Self::connect_with_cancellation(
                        self.client.clone(),
                        self.workspace_id,
                        Some(self.cursor),
                        Arc::clone(&self.cancellation),
                    ) {
                        Ok(replacement) => {
                            self.client = replacement.client;
                            self.stream = replacement.stream;
                            self.cursor = replacement.cursor;
                            return Ok(());
                        }
                        Err(error) => last_error = Some(error),
                    }
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or(ClientError::UnexpectedResponse))
    }

    fn resync(
        &mut self,
        requirement: ResyncRequirement,
    ) -> Result<SubscriptionUpdate, ClientError> {
        let snapshot = self.client.board_snapshot(self.workspace_id)?;
        self.cursor = EventCursor {
            daemon_instance_id: self.client.handshake.daemon_instance_id,
            sequence: requirement.authoritative_revision,
        };
        Ok(SubscriptionUpdate::Resynced {
            requirement,
            snapshot,
        })
    }
}

#[derive(Default)]
struct SubscriptionCancellationState {
    cancelled: AtomicBool,
    stream: Mutex<Option<TcpStream>>,
}

impl SubscriptionCancellationState {
    fn set_stream(&self, stream: &TcpStream) -> Result<(), ClientError> {
        let cancellation_stream = stream.try_clone()?;
        if self.cancelled.load(Ordering::Acquire) {
            let _ = cancellation_stream.shutdown(Shutdown::Both);
            return Err(ClientError::SubscriptionCancelled);
        }
        *self.stream.lock().expect("subscription stream lock") = Some(cancellation_stream);
        Ok(())
    }
}

#[derive(Clone)]
pub struct SubscriptionCancellation {
    state: Arc<SubscriptionCancellationState>,
}

impl SubscriptionCancellation {
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::Release);
        if let Some(stream) = self
            .state
            .stream
            .lock()
            .expect("subscription stream lock")
            .take()
        {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

fn is_disconnect(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::BrokenPipe
    )
}

#[derive(Debug, Clone, PartialEq)]
pub enum SubscriptionUpdate {
    Event(EventEnvelope),
    Resynced {
        requirement: ResyncRequirement,
        snapshot: BoardSnapshot,
    },
}

pub fn endpoint_path(database: &Path) -> PathBuf {
    database.with_extension("workboardd.json")
}

pub fn read_descriptor(path: &Path) -> Result<EndpointDescriptor, ClientError> {
    let descriptor = serde_json::from_slice(&fs::read(path)?)?;
    validate_endpoint(&descriptor)?;
    Ok(descriptor)
}

fn validate_endpoint(endpoint: &EndpointDescriptor) -> Result<(), ClientError> {
    if !endpoint.address.ip().is_loopback() {
        return Err(ClientError::InvalidEndpoint(
            "the daemon address is not loopback".to_owned(),
        ));
    }
    if endpoint.token.is_empty() || endpoint.token.chars().any(char::is_control) {
        return Err(ClientError::InvalidEndpoint(
            "the daemon credential is invalid".to_owned(),
        ));
    }
    if ![CURRENT_PROTOCOL_VERSION, PREVIOUS_PROTOCOL_VERSION].contains(&endpoint.protocol_version) {
        return Err(ClientError::IncompatibleProtocol);
    }
    Ok(())
}

pub fn remote_error(error: ProtocolError) -> ClientError {
    ClientError::Remote(Box::new(error))
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, TcpListener};
    use std::sync::mpsc;
    use std::thread;

    use workboard_client_protocol::{
        BoardSnapshot, DaemonInstanceId, HandshakeResponse, ResponseEnvelope, ResponseResult,
        ServerMessage, WorkspaceProjection, WorkspaceReference,
    };

    use super::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TestAuthenticatedRequest {
        token: String,
        request: RequestEnvelope,
    }

    #[test]
    fn endpoint_discovery_rejects_remote_addresses_and_control_credentials() {
        let remote = EndpointDescriptor {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            address: "192.0.2.1:12".parse().expect("address"),
            token: "token".to_owned(),
        };
        assert!(matches!(
            validate_endpoint(&remote),
            Err(ClientError::InvalidEndpoint(_))
        ));
        let invalid = EndpointDescriptor {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12),
            token: "token\n".to_owned(),
        };
        assert!(matches!(
            validate_endpoint(&invalid),
            Err(ClientError::InvalidEndpoint(_))
        ));
    }

    #[test]
    fn heartbeat_timeout_performs_an_authoritative_resync_query() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let workspace_id = WorkspaceId::generate();
        let repository_id = workboard_client_protocol::RepositoryId::generate();
        let daemon_instance_id = DaemonInstanceId::generate();
        let server = thread::spawn(move || {
            let (mut handshake_stream, _) = listener.accept().expect("handshake connection");
            let authenticated: TestAuthenticatedRequest =
                framing::read_frame(&mut handshake_stream).expect("handshake frame");
            assert_eq!(authenticated.token, "opaque-token");
            let handshake = ResponseEnvelope::success(
                CURRENT_PROTOCOL_VERSION,
                &authenticated.request,
                None,
                ResponseResult::Handshake(HandshakeResponse {
                    daemon_instance_id,
                    negotiated_read_version: CURRENT_PROTOCOL_VERSION,
                    compatible_command_versions: Vec::new(),
                    workspaces: vec![WorkspaceReference {
                        id: workspace_id,
                        slug: "workspace".to_owned(),
                        title: "Workspace".to_owned(),
                    }],
                    command_capabilities: Vec::new(),
                    event_version: 1,
                    heartbeat_interval_ms: 25,
                    max_frame_bytes: workboard_client_protocol::MAX_FRAME_BYTES,
                }),
                Vec::new(),
            );
            framing::write_frame(
                &mut handshake_stream,
                &ServerMessage::Response(Box::new(handshake)),
            )
            .expect("handshake response");
            let (mut subscription_stream, _) = listener.accept().expect("subscription connection");
            let authenticated: TestAuthenticatedRequest =
                framing::read_frame(&mut subscription_stream).expect("subscription frame");
            assert_eq!(authenticated.token, "opaque-token");
            let accepted = ResponseEnvelope::success(
                CURRENT_PROTOCOL_VERSION,
                &authenticated.request,
                Some(0),
                ResponseResult::SubscriptionAccepted {
                    cursor: EventCursor {
                        daemon_instance_id,
                        sequence: 0,
                    },
                },
                Vec::new(),
            );
            framing::write_frame(
                &mut subscription_stream,
                &ServerMessage::Response(Box::new(accepted)),
            )
            .expect("subscription response");
            let (mut query_stream, _) = listener.accept().expect("resync query connection");
            let authenticated: TestAuthenticatedRequest =
                framing::read_frame(&mut query_stream).expect("query frame");
            assert_eq!(authenticated.token, "opaque-token");
            let snapshot = BoardSnapshot {
                workspace: WorkspaceProjection {
                    id: workspace_id,
                    slug: "workspace".to_owned(),
                    title: "Workspace".to_owned(),
                    planning_store_repository_id: repository_id,
                },
                repositories: Vec::new(),
                epics: Vec::new(),
                features: Vec::new(),
                work_items: Vec::new(),
                documents: Vec::new(),
                checkouts: Vec::new(),
                effective_checkouts: Vec::new(),
                sessions: Vec::new(),
                associations: Vec::new(),
            };
            let response = ResponseEnvelope::success(
                CURRENT_PROTOCOL_VERSION,
                &authenticated.request,
                Some(0),
                ResponseResult::BoardSnapshot(snapshot),
                Vec::new(),
            );
            framing::write_frame(
                &mut query_stream,
                &ServerMessage::Response(Box::new(response)),
            )
            .expect("query response");
        });
        let client = WorkboardClient::connect(EndpointDescriptor {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            address,
            token: "opaque-token".to_owned(),
        })
        .expect("client")
        .with_timeout(Duration::from_millis(100));
        let mut subscription = client.subscribe(workspace_id, None).expect("subscription");
        let SubscriptionUpdate::Resynced {
            requirement,
            snapshot,
        } = subscription.next_update().expect("heartbeat resync")
        else {
            panic!("resync expected");
        };
        assert_eq!(requirement.reason, ResyncReason::HeartbeatLost);
        assert_eq!(snapshot.workspace.id, workspace_id);
        server.join().expect("fake server");
    }

    #[test]
    fn cancellation_interrupts_a_blocking_subscription_read() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let workspace_id = WorkspaceId::generate();
        let daemon_instance_id = DaemonInstanceId::generate();
        let (accepted_sender, accepted_receiver) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut handshake_stream, _) = listener.accept().expect("handshake connection");
            let authenticated: TestAuthenticatedRequest =
                framing::read_frame(&mut handshake_stream).expect("handshake frame");
            let handshake = ResponseEnvelope::success(
                CURRENT_PROTOCOL_VERSION,
                &authenticated.request,
                None,
                ResponseResult::Handshake(HandshakeResponse {
                    daemon_instance_id,
                    negotiated_read_version: CURRENT_PROTOCOL_VERSION,
                    compatible_command_versions: Vec::new(),
                    workspaces: vec![WorkspaceReference {
                        id: workspace_id,
                        slug: "workspace".to_owned(),
                        title: "Workspace".to_owned(),
                    }],
                    command_capabilities: Vec::new(),
                    event_version: 1,
                    heartbeat_interval_ms: 1_000,
                    max_frame_bytes: workboard_client_protocol::MAX_FRAME_BYTES,
                }),
                Vec::new(),
            );
            framing::write_frame(
                &mut handshake_stream,
                &ServerMessage::Response(Box::new(handshake)),
            )
            .expect("handshake response");
            let (mut subscription_stream, _) = listener.accept().expect("subscription connection");
            let authenticated: TestAuthenticatedRequest =
                framing::read_frame(&mut subscription_stream).expect("subscription frame");
            let accepted = ResponseEnvelope::success(
                CURRENT_PROTOCOL_VERSION,
                &authenticated.request,
                Some(0),
                ResponseResult::SubscriptionAccepted {
                    cursor: EventCursor {
                        daemon_instance_id,
                        sequence: 0,
                    },
                },
                Vec::new(),
            );
            framing::write_frame(
                &mut subscription_stream,
                &ServerMessage::Response(Box::new(accepted)),
            )
            .expect("subscription response");
            accepted_sender.send(()).expect("accepted signal");
            thread::sleep(Duration::from_secs(1));
        });
        let client = WorkboardClient::connect(EndpointDescriptor {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            address,
            token: "opaque-token".to_owned(),
        })
        .expect("client");
        let mut subscription = client.subscribe(workspace_id, None).expect("subscription");
        let cancellation = subscription.cancellation_handle();
        let worker = thread::spawn(move || subscription.next_update());
        accepted_receiver.recv().expect("accepted subscription");
        cancellation.cancel();
        assert!(matches!(
            worker.join().expect("subscription worker"),
            Err(ClientError::SubscriptionCancelled)
        ));
        server.join().expect("fake server");
    }
}
