use std::fs;
use std::path::Path;

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn protocol_and_client_dependency_edges_remain_transport_only() {
    let client = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates = client.parent().expect("crates directory");
    let protocol_manifest = read(&crates.join("workboard-client-protocol/Cargo.toml"));
    let client_manifest = read(&client.join("Cargo.toml"));
    for forbidden in [
        "workboard-application",
        "workboard-daemon",
        "rusqlite",
        "workboard-adapter",
        "tauri",
    ] {
        assert!(!protocol_manifest.contains(forbidden), "{forbidden}");
        assert!(!client_manifest.contains(forbidden), "{forbidden}");
    }
}

#[test]
fn migrated_cli_board_and_show_paths_have_no_storage_fallback() {
    let client = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates = client.parent().expect("crates directory");
    let cli = read(&crates.join("workboard-cli/src/lib.rs"));
    let interactive_start = cli
        .find("fn run_interactive_board")
        .expect("interactive board");
    let interactive_end = cli[interactive_start..]
        .find("\nfn output")
        .map(|offset| interactive_start + offset)
        .expect("interactive board end");
    let interactive = &cli[interactive_start..interactive_end];
    assert!(interactive.contains("client_snapshot"));
    assert!(!interactive.contains("WorkboardApplication::open"));
    assert!(cli.contains("Some(Command::Show) => unreachable!()"));
    assert!(cli.contains("if cli.command.is_none()"));
}

#[test]
fn public_protocol_sources_contain_no_credential_field() {
    let client = Path::new(env!("CARGO_MANIFEST_DIR"));
    let protocol = client
        .parent()
        .expect("crates directory")
        .join("workboard-client-protocol/src");
    for source in ["identity.rs", "projection.rs", "wire.rs"] {
        let contents = read(&protocol.join(source));
        assert!(!contents.contains("pub token:"), "{source}");
        assert!(!contents.contains("pub credential:"), "{source}");
    }
}
