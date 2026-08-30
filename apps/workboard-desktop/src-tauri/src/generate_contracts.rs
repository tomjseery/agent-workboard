use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use ts_rs::{Config, TS};
use workboard_client_protocol::generation::{fixture_bytes, typescript_declarations};
use workboard_client_protocol::{CURRENT_PROTOCOL_VERSION, PREVIOUS_PROTOCOL_VERSION};
use workboard_desktop::{
    BootstrapHandshake, BootstrapState, BridgeError, ExecuteRequest, QueryRequest,
    SubscribeRequest, SubscriptionMessage, SubscriptionReceipt, SubscriptionTarget,
};

const CONTRACTS_FILE: &str = "contracts.ts";
const INDEX_FILE: &str = "index.ts";
const CURRENT_FIXTURE_FILE: &str = "conformance-current.json";
const PREVIOUS_FIXTURE_FILE: &str = "conformance-previous.json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "check".to_owned());
    let output = generated_directory();
    match mode.as_str() {
        "write" => write_artifacts(&output, &artifacts()?)?,
        "check" => check(&output)?,
        _ => return Err(format!("unknown generation mode: {mode}").into()),
    }
    Ok(())
}

fn generated_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/core/generated")
}

fn artifacts() -> Result<BTreeMap<&'static str, Vec<u8>>, Box<dyn std::error::Error>> {
    let mut contracts = typescript_declarations();
    contracts.push_str(&bridge_declarations());
    assert_safe_contract(&contracts)?;

    Ok(BTreeMap::from([
        (CONTRACTS_FILE, contracts.into_bytes()),
        (INDEX_FILE, b"export type * from './contracts';\n".to_vec()),
        (
            CURRENT_FIXTURE_FILE,
            fixture_bytes(CURRENT_PROTOCOL_VERSION),
        ),
        (
            PREVIOUS_FIXTURE_FILE,
            fixture_bytes(PREVIOUS_PROTOCOL_VERSION),
        ),
    ]))
}

fn bridge_declarations() -> String {
    let config = Config::default();
    let mut declarations = Vec::new();
    macro_rules! declaration {
        ($type:ty) => {
            declarations.push((
                <$type as TS>::name(&config),
                <$type as TS>::decl(&config).replace("bigint", "number"),
            ));
        };
    }

    declaration!(BootstrapState);
    declaration!(SubscriptionTarget);
    declaration!(BootstrapHandshake);
    declaration!(QueryRequest);
    declaration!(ExecuteRequest);
    declaration!(SubscribeRequest);
    declaration!(SubscriptionReceipt);
    declaration!(SubscriptionMessage);
    declaration!(BridgeError);

    declarations.sort_by(|left, right| left.0.cmp(&right.0));
    declarations
        .into_iter()
        .map(|(_, declaration)| format!("export {declaration}\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn write_artifacts(
    output: &Path,
    artifacts: &BTreeMap<&str, Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(output)?;
    for (name, bytes) in artifacts {
        fs::write(output.join(name), bytes)?;
    }
    Ok(())
}

fn check(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let first = TempDir::new()?;
    let second = TempDir::new()?;
    let first_artifacts = artifacts()?;
    let second_artifacts = artifacts()?;
    write_artifacts(first.path(), &first_artifacts)?;
    write_artifacts(second.path(), &second_artifacts)?;
    if first_artifacts != second_artifacts {
        return Err("two clean generation runs were not byte-identical".into());
    }

    let expected_names = first_artifacts.keys().copied().collect::<Vec<_>>();
    let mut actual_names = if output.exists() {
        fs::read_dir(output)?
            .map(|entry| {
                entry?
                    .file_name()
                    .into_string()
                    .map_err(|_| std::io::Error::other("non-UTF-8 generated filename"))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    actual_names.sort();
    if actual_names != expected_names {
        return Err("generated contract file set has drifted".into());
    }

    for (name, expected) in first_artifacts {
        let actual = fs::read(output.join(name))?;
        if actual != expected {
            return Err(format!("generated contract has drifted: {name}").into());
        }
    }
    Ok(())
}

fn assert_safe_contract(contracts: &str) -> Result<(), Box<dyn std::error::Error>> {
    for forbidden in [
        " token:",
        " credential:",
        " credentials:",
        " password:",
        " secret:",
        " path:",
        " paths:",
        " url:",
        " urls:",
        " socket:",
        " commandLine:",
        " providerCommand:",
        " internalDiagnostic:",
        " internalDiagnostics:",
        " gitCommonDirectory:",
    ] {
        if contracts.contains(forbidden) {
            return Err(format!("forbidden generated contract field: {}", forbidden.trim()).into());
        }
    }
    Ok(())
}
