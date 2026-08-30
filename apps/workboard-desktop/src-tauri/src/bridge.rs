use tauri::ipc::Channel;
use tauri::{Runtime, State, WebviewWindow};
use workboard_client_protocol::ResponseEnvelope;

use crate::runtime::DesktopRuntime;
use crate::types::{
    BootstrapHandshake, BridgeError, ExecuteRequest, QueryRequest, SubscribeRequest,
    SubscriptionMessage, SubscriptionReceipt,
};

fn authorize_window<R: Runtime>(window: &WebviewWindow<R>) -> Result<String, BridgeError> {
    if window.label() != "main" {
        return Err(BridgeError::forbidden_window());
    }
    Ok(window.label().to_owned())
}

async fn run_blocking<T: Send + 'static>(
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, BridgeError> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| BridgeError::disconnected())
}

#[tauri::command]
pub async fn workboard_handshake<R: Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, DesktopRuntime>,
) -> Result<BootstrapHandshake, BridgeError> {
    authorize_window(&webview_window)?;
    let runtime = state.inner().clone();
    run_blocking(move || runtime.handshake()).await
}

#[tauri::command]
pub async fn workboard_query<R: Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, DesktopRuntime>,
    request: QueryRequest,
) -> Result<ResponseEnvelope, BridgeError> {
    authorize_window(&webview_window)?;
    let runtime = state.inner().clone();
    run_blocking(move || runtime.query(request)).await?
}

#[tauri::command]
pub async fn workboard_execute<R: Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, DesktopRuntime>,
    request: ExecuteRequest,
) -> Result<ResponseEnvelope, BridgeError> {
    authorize_window(&webview_window)?;
    let runtime = state.inner().clone();
    run_blocking(move || runtime.execute(request)).await?
}

#[tauri::command]
pub async fn workboard_subscribe<R: Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, DesktopRuntime>,
    request: SubscribeRequest,
    on_message: Channel<SubscriptionMessage>,
) -> Result<SubscriptionReceipt, BridgeError> {
    let window_label = authorize_window(&webview_window)?;
    let runtime = state.inner().clone();
    run_blocking(move || runtime.subscribe(window_label, request, on_message)).await?
}
