use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use workboard_core::{
    HierarchyOwner, ManagedSessionRole, Tool, WORKBOARD_BUNDLE_ENV, WORKBOARD_CHECKOUT_ENV,
    WORKBOARD_OWNER_ENV, WORKBOARD_REPOSITORY_ENV, WORKBOARD_SESSION_ROLE_ENV,
};

use crate::error::AppError;
use crate::integration::{owned_hook_configuration, provider_configuration_file};
use crate::workflow_contract::{bundle_assets, generated_skill};

pub const CAPABILITY_BUNDLE_VERSION: &str = "agent-workboard/capability-bundle-v1";

const SKILLS_DIRECTORY: &str = "skills";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleContext {
    pub tool: Tool,
    pub role: ManagedSessionRole,
    pub owner: HierarchyOwner,
    pub repository: String,
    pub checkout: PathBuf,
    pub workboard_executable: PathBuf,
    pub database: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareCapabilityBundle {
    pub root: PathBuf,
    pub provider_home: PathBuf,
    pub context: BundleContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCapabilityBundle {
    pub root: PathBuf,
    pub transcript_root: PathBuf,
    pub environment: Vec<(String, String)>,
    pub digest: String,
    pub version: &'static str,
}

pub const fn configuration_environment(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "CLAUDE_CONFIG_DIR",
        Tool::Codex => "CODEX_HOME",
    }
}

const fn transcript_directory(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "projects",
        Tool::Codex => "sessions",
    }
}

const fn credential_file(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => ".credentials.json",
        Tool::Codex => "auth.json",
    }
}

const fn referenced_files(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Claude => &[],
        Tool::Codex => &["config.toml", "requirements.toml"],
    }
}

pub fn prepare_bundle(
    request: &PrepareCapabilityBundle,
) -> Result<PreparedCapabilityBundle, AppError> {
    let PrepareCapabilityBundle {
        root,
        provider_home,
        context,
    } = request;
    if !root.is_absolute() {
        return Err(AppError::CapabilityBundlePathInvalid(root.clone()));
    }

    retire_bundle(root)?;
    create_directory(root)?;

    let transcript_root = root.join(transcript_directory(context.tool));
    create_directory(&transcript_root)?;

    let mut digest_input: BTreeMap<String, String> = BTreeMap::new();

    let skills = root.join(SKILLS_DIRECTORY);
    create_directory(&skills)?;
    for asset in bundle_assets(context.role) {
        let body = generated_skill(asset, &context.workboard_executable)?;
        let directory = skills.join(asset.name);
        create_directory(&directory)?;
        write_owned_file(&directory.join("SKILL.md"), body.as_bytes())?;
        digest_input.insert(format!("{SKILLS_DIRECTORY}/{}/SKILL.md", asset.name), body);
    }

    let hooks = owned_hook_configuration(
        context.tool,
        &context.workboard_executable,
        &context.database,
    )?;
    let hooks = serde_json::to_string_pretty(&hooks)?;
    let configuration = root.join(provider_configuration_file(context.tool));
    write_owned_file(&configuration, hooks.as_bytes())?;
    digest_input.insert(
        provider_configuration_file(context.tool).to_owned(),
        hooks.clone(),
    );

    link_provider_credential(provider_home, root, context.tool)?;
    for name in referenced_files(context.tool) {
        let source = provider_home.join(name);
        if source.is_file() {
            link_file(&source, &root.join(name))?;
        }
    }

    Ok(PreparedCapabilityBundle {
        root: root.clone(),
        transcript_root,
        environment: bundle_environment(root, context)?,
        digest: bundle_digest(&digest_input),
        version: CAPABILITY_BUNDLE_VERSION,
    })
}

pub fn retire_bundle(root: &Path) -> Result<(), AppError> {
    if !root.is_absolute() {
        return Err(AppError::CapabilityBundlePathInvalid(root.to_path_buf()));
    }
    if !root.is_dir() {
        return Ok(());
    }
    let contained = canonical_root(root)?;
    for tool in [Tool::Claude, Tool::Codex] {
        remove_contained_file(&contained, &root.join(provider_configuration_file(tool)))?;
        remove_contained_file(&contained, &root.join(credential_file(tool)))?;
        for name in referenced_files(tool) {
            remove_contained_file(&contained, &root.join(name))?;
        }
    }
    let skills = root.join(SKILLS_DIRECTORY);
    if skills.is_dir() {
        let skills = canonical_path(&skills)?;
        if !skills.starts_with(&contained) {
            return Err(AppError::CapabilityBundleEscape(skills));
        }
        fs::remove_dir_all(&skills).map_err(|source| AppError::CapabilityBundleIo {
            operation: "removing capability skills",
            path: skills,
            source,
        })?;
    }
    Ok(())
}

fn bundle_environment(
    root: &Path,
    context: &BundleContext,
) -> Result<Vec<(String, String)>, AppError> {
    let (owner_kind, owner_id) = match context.owner {
        HierarchyOwner::Workspace(id) => ("workspace", id.to_string()),
        HierarchyOwner::Epic(id) => ("epic", id.to_string()),
        HierarchyOwner::Feature(id) => ("feature", id.to_string()),
        HierarchyOwner::WorkItem(id) => ("work_item", id.to_string()),
    };
    Ok(vec![
        (
            configuration_environment(context.tool).to_owned(),
            path_text(root)?.to_owned(),
        ),
        (WORKBOARD_BUNDLE_ENV.to_owned(), path_text(root)?.to_owned()),
        (
            WORKBOARD_SESSION_ROLE_ENV.to_owned(),
            role_name(context.role).to_owned(),
        ),
        (
            WORKBOARD_OWNER_ENV.to_owned(),
            format!("{owner_kind}:{owner_id}"),
        ),
        (
            WORKBOARD_REPOSITORY_ENV.to_owned(),
            context.repository.clone(),
        ),
        (
            WORKBOARD_CHECKOUT_ENV.to_owned(),
            path_text(&context.checkout)?.to_owned(),
        ),
    ])
}

const fn role_name(role: ManagedSessionRole) -> &'static str {
    match role {
        ManagedSessionRole::WorkspacePlanning => "workspace_planning",
        ManagedSessionRole::EpicNavigation => "epic_navigation",
        ManagedSessionRole::FeaturePlanning => "feature_planning",
        ManagedSessionRole::WorkItemExecution => "work_item_execution",
        ManagedSessionRole::Debugging => "debugging",
        ManagedSessionRole::Review => "review",
    }
}

fn bundle_digest(assets: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(CAPABILITY_BUNDLE_VERSION.as_bytes());
    for (path, body) in assets {
        hasher.update(b"\0");
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        hasher.update(body.as_bytes());
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn link_provider_credential(provider_home: &Path, root: &Path, tool: Tool) -> Result<(), AppError> {
    let name = credential_file(tool);
    let source = provider_home.join(name);
    if !source.is_file() {
        return Err(AppError::CapabilityBundleCredentialMissing {
            tool: match tool {
                Tool::Claude => "Claude",
                Tool::Codex => "Codex",
            },
            path: source,
        });
    }
    link_file(&source, &root.join(name))
}

fn link_file(source: &Path, destination: &Path) -> Result<(), AppError> {
    if destination.exists() {
        fs::remove_file(destination).map_err(|error| AppError::CapabilityBundleIo {
            operation: "replacing a referenced provider file",
            path: destination.to_path_buf(),
            source: error,
        })?;
    }
    match fs::hard_link(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source, destination).map_err(|error| AppError::CapabilityBundleIo {
                operation: "referencing a provider file",
                path: destination.to_path_buf(),
                source: error,
            })?;
            Ok(())
        }
    }
}

fn write_owned_file(path: &Path, contents: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| AppError::CapabilityBundleIo {
        operation: "resolving a capability asset directory",
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing parent directory"),
    })?;
    let mut staged =
        NamedTempFile::new_in(parent).map_err(|source| AppError::CapabilityBundleIo {
            operation: "staging a capability asset",
            path: path.to_path_buf(),
            source,
        })?;
    staged
        .write_all(contents)
        .and_then(|()| staged.as_file_mut().sync_all())
        .map_err(|source| AppError::CapabilityBundleIo {
            operation: "writing a capability asset",
            path: path.to_path_buf(),
            source,
        })?;
    staged
        .persist(path)
        .map_err(|error| AppError::CapabilityBundleIo {
            operation: "publishing a capability asset",
            path: path.to_path_buf(),
            source: error.error,
        })?;
    Ok(())
}

fn create_directory(path: &Path) -> Result<(), AppError> {
    fs::create_dir_all(path).map_err(|source| AppError::CapabilityBundleIo {
        operation: "creating a capability bundle directory",
        path: path.to_path_buf(),
        source,
    })
}

fn remove_contained_file(contained: &Path, path: &Path) -> Result<(), AppError> {
    if !path.is_file() {
        return Ok(());
    }
    let resolved = canonical_path(path)?;
    if !resolved.starts_with(contained) {
        return Err(AppError::CapabilityBundleEscape(resolved));
    }
    fs::remove_file(&resolved).map_err(|source| AppError::CapabilityBundleIo {
        operation: "removing a capability asset",
        path: resolved,
        source,
    })
}

fn canonical_root(root: &Path) -> Result<PathBuf, AppError> {
    canonical_path(root)
}

fn canonical_path(path: &Path) -> Result<PathBuf, AppError> {
    fs::canonicalize(path).map_err(|source| AppError::CapabilityBundleIo {
        operation: "resolving a capability bundle path",
        path: path.to_path_buf(),
        source,
    })
}

fn path_text(path: &Path) -> Result<&str, AppError> {
    path.to_str()
        .filter(|value| !value.chars().any(char::is_control))
        .ok_or_else(|| AppError::CapabilityBundlePathInvalid(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;
    use workboard_core::{HierarchyOwner, ManagedSessionRole, Tool, WorkspaceId};

    use super::{
        BundleContext, PrepareCapabilityBundle, configuration_environment, prepare_bundle,
        retire_bundle,
    };
    use crate::error::AppError;

    struct Fixture {
        _directory: TempDir,
        root: PathBuf,
        provider_home: PathBuf,
        database: PathBuf,
        executable: PathBuf,
    }

    fn fixture(tool: Tool) -> Fixture {
        let directory = TempDir::new().expect("fixture directory");
        let provider_home = directory.path().join("provider-home");
        fs::create_dir_all(&provider_home).expect("provider home");
        let credential = match tool {
            Tool::Claude => ".credentials.json",
            Tool::Codex => "auth.json",
        };
        fs::write(provider_home.join(credential), b"{}").expect("provider credential");
        let database = directory.path().join("workboard.sqlite");
        fs::write(&database, b"").expect("database fixture");
        let executable = directory.path().join("workboard.exe");
        fs::write(&executable, b"").expect("executable fixture");
        Fixture {
            root: directory.path().join("managed-sessions").join("intent-one"),
            provider_home,
            database,
            executable,
            _directory: directory,
        }
    }

    fn request(fixture: &Fixture, tool: Tool, role: ManagedSessionRole) -> PrepareCapabilityBundle {
        PrepareCapabilityBundle {
            root: fixture.root.clone(),
            provider_home: fixture.provider_home.clone(),
            context: BundleContext {
                tool,
                role,
                owner: HierarchyOwner::Workspace(WorkspaceId::generate()),
                repository: "concertable".to_owned(),
                checkout: fixture.root.clone(),
                workboard_executable: fixture.executable.clone(),
                database: fixture.database.clone(),
            },
        }
    }

    fn installed_skills(root: &Path) -> BTreeSet<String> {
        let skills = root.join("skills");
        if !skills.is_dir() {
            return BTreeSet::new();
        }
        fs::read_dir(skills)
            .expect("read skills")
            .map(|entry| entry.expect("skill entry").file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_launch_receives_exactly_the_skills_its_role_allows() {
        let fixture = fixture(Tool::Claude);
        let prepared = prepare_bundle(&request(
            &fixture,
            Tool::Claude,
            ManagedSessionRole::WorkspacePlanning,
        ))
        .expect("prepared bundle");

        assert_eq!(
            installed_skills(&prepared.root),
            BTreeSet::from([
                "workboard-research-import".to_owned(),
                "workboard-epic-proposal".to_owned(),
                "workboard-feature-proposal".to_owned(),
            ])
        );
        assert!(!prepared.root.join("skills/workboard-checkpoint").exists());
        assert!(!prepared.root.join("skills/workboard-publication").exists());
        assert!(prepared.root.join("settings.json").is_file());
        assert!(prepared.root.join("projects").is_dir());
        assert!(prepared.root.join(".credentials.json").is_file());
        assert!(prepared.digest.starts_with("sha256:"));
    }

    #[test]
    fn the_child_environment_carries_the_bundle_role_owner_repository_and_checkout() {
        let fixture = fixture(Tool::Codex);
        let prepared = prepare_bundle(&request(
            &fixture,
            Tool::Codex,
            ManagedSessionRole::WorkItemExecution,
        ))
        .expect("prepared bundle");

        let environment = prepared
            .environment
            .iter()
            .cloned()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            environment.get(configuration_environment(Tool::Codex)),
            Some(&prepared.root.to_string_lossy().into_owned())
        );
        assert_eq!(
            environment.get("WORKBOARD_SESSION_ROLE"),
            Some(&"work_item_execution".to_owned())
        );
        assert_eq!(
            environment.get("WORKBOARD_REPOSITORY"),
            Some(&"concertable".to_owned())
        );
        assert!(
            environment
                .get("WORKBOARD_OWNER")
                .is_some_and(|owner| owner.starts_with("workspace:"))
        );
        assert!(environment.contains_key("WORKBOARD_CHECKOUT"));
        assert!(prepared.root.join("hooks.json").is_file());
        assert!(prepared.root.join("sessions").is_dir());
    }

    #[test]
    fn the_same_role_and_contract_produce_a_stable_bundle_identity() {
        let first = fixture(Tool::Claude);
        let second = fixture(Tool::Claude);
        let first = prepare_bundle(&request(
            &first,
            Tool::Claude,
            ManagedSessionRole::FeaturePlanning,
        ))
        .expect("first bundle");
        let mut second_request =
            request(&second, Tool::Claude, ManagedSessionRole::FeaturePlanning);
        second_request.context.workboard_executable = first.root.join("..").join("workboard.exe");
        let different_role = prepare_bundle(&request(
            &second,
            Tool::Claude,
            ManagedSessionRole::WorkItemExecution,
        ))
        .expect("second bundle");

        assert_ne!(first.digest, different_role.digest);
        assert_eq!(first.version, different_role.version);
    }

    #[test]
    fn retiring_a_bundle_removes_capabilities_and_keeps_transcripts() {
        let fixture = fixture(Tool::Claude);
        let prepared = prepare_bundle(&request(
            &fixture,
            Tool::Claude,
            ManagedSessionRole::WorkspacePlanning,
        ))
        .expect("prepared bundle");
        let transcript = prepared.transcript_root.join("session.jsonl");
        fs::write(&transcript, b"{}").expect("transcript fixture");

        retire_bundle(&prepared.root).expect("retire the bundle");

        assert!(!prepared.root.join("skills").exists());
        assert!(!prepared.root.join("settings.json").exists());
        assert!(!prepared.root.join(".credentials.json").exists());
        assert!(transcript.is_file(), "transcripts must survive close");

        retire_bundle(&prepared.root).expect("retiring twice is idempotent");
    }

    #[test]
    fn a_launch_without_provider_credentials_fails_closed() {
        let fixture = fixture(Tool::Claude);
        fs::remove_file(fixture.provider_home.join(".credentials.json"))
            .expect("remove the provider credential");

        let outcome = prepare_bundle(&request(
            &fixture,
            Tool::Claude,
            ManagedSessionRole::WorkspacePlanning,
        ));

        assert!(matches!(
            outcome,
            Err(AppError::CapabilityBundleCredentialMissing { .. })
        ));
    }

    #[test]
    fn a_relative_bundle_root_is_rejected() {
        let fixture = fixture(Tool::Claude);
        let mut request = request(
            &fixture,
            Tool::Claude,
            ManagedSessionRole::WorkspacePlanning,
        );
        request.root = PathBuf::from("relative-bundle");

        assert!(matches!(
            prepare_bundle(&request),
            Err(AppError::CapabilityBundlePathInvalid(_))
        ));
        assert!(matches!(
            retire_bundle(Path::new("relative-bundle")),
            Err(AppError::CapabilityBundlePathInvalid(_))
        ));
    }
}
