#![forbid(unsafe_code)]

mod bridge;
mod connection;
mod runtime;
mod subscriptions;
mod types;

use tauri::{Builder, Manager, Runtime, WindowEvent};

pub use runtime::DesktopRuntime;
pub use types::{
    BootstrapHandshake, BootstrapState, BridgeError, ExecuteRequest, QueryRequest,
    SubscribeRequest, SubscriptionMessage, SubscriptionReceipt, SubscriptionTarget,
};

pub fn configure<R: Runtime>(builder: Builder<R>, runtime: DesktopRuntime) -> Builder<R> {
    builder
        .manage(runtime)
        .invoke_handler(tauri::generate_handler![
            bridge::workboard_handshake,
            bridge::workboard_query,
            bridge::workboard_execute,
            bridge::workboard_subscribe,
        ])
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::Destroyed) {
                window
                    .state::<DesktopRuntime>()
                    .cancel_window(window.label());
            }
        })
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = DesktopRuntime::system()
        .map_err(|error| std::io::Error::other(format!("{}: {}", error.code, error.message)))?;
    configure(tauri::Builder::default(), runtime)
        .run(tauri::generate_context!())
        .map_err(Into::into)
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
