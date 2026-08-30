use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

fn desktop() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("desktop directory")
        .to_path_buf()
}

fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn json(path: impl AsRef<Path>) -> Value {
    serde_json::from_str(&read(path)).expect("valid JSON")
}

#[test]
fn tauri_configuration_is_local_isolated_and_content_restricted() {
    let desktop = desktop();
    let config = json(desktop.join("src-tauri/tauri.conf.json"));
    assert_eq!(config["app"]["withGlobalTauri"], false);
    let windows = config["app"]["windows"].as_array().expect("windows");
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0]["label"], "main");
    assert_eq!(windows[0]["devtools"], false);
    assert_eq!(config["build"]["devUrl"], "http://localhost:1420");
    assert_eq!(config["build"]["frontendDist"], "../dist");
    let security = &config["app"]["security"];
    assert_eq!(security["pattern"]["use"], "isolation");
    assert_eq!(security["pattern"]["options"]["dir"], "../isolation");
    assert_eq!(security["assetProtocol"]["enable"], false);
    assert_eq!(security["assetProtocol"]["scope"], json!([]));
    let csp = security["csp"].as_str().expect("CSP");
    for required in [
        "default-src 'self'",
        "script-src 'self'",
        "connect-src ipc: http://ipc.localhost",
        "object-src 'none'",
        "base-uri 'none'",
        "form-action 'none'",
    ] {
        assert!(csp.contains(required), "{required}");
    }
    for forbidden in [
        "unsafe-eval",
        "https://",
        "asset:",
        "http://asset.localhost",
        "ws:",
        "wss:",
        "data: script",
    ] {
        assert!(!csp.contains(forbidden), "{forbidden}");
    }
}

#[test]
fn one_exact_main_webview_capability_has_only_custom_bridge_permissions() {
    let desktop = desktop();
    let capabilities = fs::read_dir(desktop.join("src-tauri/capabilities"))
        .expect("capabilities")
        .map(|entry| entry.expect("capability entry").path())
        .collect::<Vec<_>>();
    assert_eq!(capabilities.len(), 1);
    let capability = json(&capabilities[0]);
    assert_eq!(capability["local"], true);
    assert_eq!(capability["webviews"], json!(["main"]));
    assert!(capability.get("windows").is_none());
    assert!(capability.get("remote").is_none());
    let actual = capability["permissions"]
        .as_array()
        .expect("permissions")
        .iter()
        .map(|permission| permission.as_str().expect("permission"))
        .collect::<BTreeSet<_>>();
    let expected = [
        "allow-workboard-execute",
        "allow-workboard-handshake",
        "allow-workboard-query",
        "allow-workboard-subscribe",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn app_manifest_is_exact_and_no_forbidden_plugin_is_registered() {
    let desktop = desktop();
    let build = read(desktop.join("src-tauri/build.rs"));
    let library = read(desktop.join("src-tauri/src/lib.rs"));
    for command in [
        "workboard_handshake",
        "workboard_query",
        "workboard_execute",
        "workboard_subscribe",
    ] {
        assert_eq!(build.matches(&format!("\"{command}\"")).count(), 1);
        assert_eq!(library.matches(&format!("bridge::{command}")).count(), 1);
    }
    assert!(!library.contains(".plugin("));
    let manifest = read(desktop.join("src-tauri/Cargo.toml"));
    for forbidden in [
        "tauri-plugin-sql",
        "tauri-plugin-fs",
        "tauri-plugin-shell",
        "tauri-plugin-http",
        "tauri-plugin-process",
        "tauri-plugin-opener",
        "tauri-plugin-updater",
        "tauri-plugin-dialog",
    ] {
        assert!(!manifest.contains(forbidden), "{forbidden}");
    }
}

#[test]
fn isolation_and_frontend_have_no_alternate_native_or_network_bridge() {
    let desktop = desktop();
    let isolation = read(desktop.join("isolation/index.js"));
    for command in [
        "workboard_handshake",
        "workboard_query",
        "workboard_execute",
        "workboard_subscribe",
    ] {
        assert!(isolation.contains(command));
    }
    for forbidden in ["fetch(", "WebSocket", "console.", "http://", "https://"] {
        assert!(!isolation.contains(forbidden), "{forbidden}");
    }
    let source = desktop.join("src");
    let mut invoke_owners = Vec::new();
    for entry in walk(&source) {
        let contents = read(&entry);
        for forbidden in [
            "@tauri-apps/plugin-",
            "WebSocket",
            "fetch(",
            "XMLHttpRequest",
            "window.__TAURI__",
            "console.",
        ] {
            assert!(
                !contents.contains(forbidden),
                "{}: {forbidden}",
                entry.display()
            );
        }
        if contents.contains("invoke(") || contents.contains("invoke<") {
            invoke_owners.push(entry);
        }
    }
    assert_eq!(invoke_owners, vec![source.join("core").join("bridge.ts")]);
}

#[test]
fn rust_shell_dependency_and_source_edges_remain_client_only() {
    let desktop = desktop();
    let manifest = read(desktop.join("src-tauri/Cargo.toml"));
    for forbidden in [
        "workboard-application",
        "workboard-daemon",
        "workboard-core",
        "workboard-native",
        "workboard-adapter",
        "rusqlite",
        "git2",
    ] {
        assert!(!manifest.contains(forbidden), "{forbidden}");
    }
    for entry in walk(&desktop.join("src-tauri/src")) {
        if entry
            .file_name()
            .is_some_and(|name| name == "test_support.rs" || name == "tests.rs")
        {
            continue;
        }
        let contents = read(&entry);
        for forbidden in ["println!(", "eprintln!(", "dbg!(", "log::", "tracing::"] {
            assert!(
                !contents.contains(forbidden),
                "{}: {forbidden}",
                entry.display()
            );
        }
    }
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).expect("source directory") {
            let path = entry.expect("source entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("rs" | "ts" | "tsx")
            ) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}
