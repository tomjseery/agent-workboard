use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tempfile::{Builder, NamedTempFile};
use workboard_core::Tool;

use crate::error::AppError;
use crate::workflow_contract::{
    WORKFLOW_CONTRACT_VERSION, generated_continue_roadmap_shim, generated_skill,
};

pub const INTEGRATION_OWNER: &str = "agent-workboard/native-integration-v1";
const MAX_CONFIGURATION_BYTES: u64 = 1024 * 1024;
const MAX_PRETTY_CONFIGURATION_BYTES: usize = 1024 * 1024;
pub const ADAPTER_VERSION: &str = "native-hook-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationOperation {
    Status,
    Preview,
    Install,
    Repair,
    Disable,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationRequest {
    pub tool: Tool,
    pub native_home: PathBuf,
    pub workboard_executable: PathBuf,
    pub operation: IntegrationOperation,
    #[serde(default)]
    pub preview_operation: Option<IntegrationOperation>,
    #[serde(default)]
    pub confirmation: Option<IntegrationConfirmation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationConfirmation {
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationState {
    NotInstalled,
    Installed,
    NeedsRepair,
    Disabled,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationCapability {
    pub available: bool,
    pub code: String,
    pub message: String,
    pub contract_version: String,
    pub requires_native_review: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub schema_version: u32,
    pub tool: Tool,
    pub configuration_path: PathBuf,
    pub workflow_contract_path: PathBuf,
    pub workflow_contract_version: String,
    pub state: IntegrationState,
    pub enabled_in_workboard: bool,
    pub adapter_version: Option<String>,
    pub first_observed_at: Option<String>,
    pub last_observed_at: Option<String>,
    pub last_hook_observed_at: Option<String>,
    pub last_app_server_observed_at: Option<String>,
    pub capability: IntegrationCapability,
    pub available_operations: Vec<IntegrationOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationPreview {
    pub schema_version: u32,
    pub status: IntegrationStatus,
    pub owned_configuration: Value,
    pub contents: Option<String>,
    pub workflow_contract_contents: Option<String>,
    pub confirmation_token: String,
    pub will_change: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationOutcome {
    pub schema_version: u32,
    pub operation: IntegrationOperation,
    pub changed: bool,
    pub backup_path: Option<PathBuf>,
    pub workflow_backup_path: Option<PathBuf>,
    pub status: IntegrationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum IntegrationResponse {
    Status { status: IntegrationStatus },
    Preview { preview: IntegrationPreview },
    Mutation { outcome: IntegrationOutcome },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationRegistration {
    pub enabled: bool,
    pub adapter_version: String,
    pub first_observed_at: Option<String>,
    pub last_observed_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntegrationObservations {
    pub first_observed_at: Option<String>,
    pub last_observed_at: Option<String>,
    pub last_hook_observed_at: Option<String>,
    pub last_app_server_observed_at: Option<String>,
}

pub struct IntegrationPlan {
    tool: Tool,
    configuration_path: PathBuf,
    current_bytes: Option<Vec<u8>>,
    current_value: Option<Value>,
    installed_value: Option<Value>,
    removed_value: Option<Value>,
    owned_configuration: Value,
    workflow_contract_path: PathBuf,
    workflow_current: Option<Vec<u8>>,
    workflow_installed: Vec<u8>,
    compatibility_path: PathBuf,
    compatibility_current: Option<Vec<u8>>,
    compatibility_installed: Vec<u8>,
    capability: IntegrationCapability,
}

pub struct PreparedIntegrationPreview {
    pub preview: IntegrationPreview,
    pub confirmation_digest: String,
}

impl PreparedIntegrationPreview {
    pub fn into_preview(mut self, confirmation_token: String) -> IntegrationPreview {
        self.preview.confirmation_token = confirmation_token;
        self.preview
    }
}

impl IntegrationPlan {
    pub fn load(request: &IntegrationRequest, database: &Path) -> Result<Self, AppError> {
        validate_absolute(&request.native_home, "native integration home")?;
        let executable = canonical_file(&request.workboard_executable, "workboard executable")?;
        let database = canonical_file(database, "Workboard database")?;
        let configuration_path = configuration_path(request.tool, &request.native_home);
        let owned_configuration = owned_configuration(request.tool, &executable, &database)?;
        let workflow_contract_path = request
            .native_home
            .join("skills")
            .join("agent-workboard")
            .join("SKILL.md");
        let workflow_installed = generated_skill(&executable)?.into_bytes();
        let workflow_current = workflow_contract_path
            .is_file()
            .then(|| read_bounded(&workflow_contract_path))
            .transpose()?;
        let compatibility_path = request
            .native_home
            .join("skills")
            .join("continue-roadmap")
            .join("SKILL.md");
        let compatibility_installed = generated_continue_roadmap_shim(&executable)?.into_bytes();
        let compatibility_current = compatibility_path
            .is_file()
            .then(|| read_bounded(&compatibility_path))
            .transpose()?;
        let current = read_json_configuration(&configuration_path);
        let mut capability = capability(request.tool, &request.native_home, current.as_ref());
        if workflow_current
            .as_deref()
            .is_some_and(|contents| !is_owned_workflow_contract(contents))
        {
            capability = unavailable_capability(
                request.tool,
                "workflow_contract_conflict",
                format!(
                    "the workflow integration path is owned by another file: {}",
                    workflow_contract_path.display()
                ),
            );
        }
        if compatibility_current
            .as_deref()
            .is_some_and(|contents| !is_owned_workflow_contract(contents))
        {
            capability = unavailable_capability(
                request.tool,
                "workflow_contract_conflict",
                format!(
                    "the compatibility integration path is owned by another file: {}",
                    compatibility_path.display()
                ),
            );
        }
        let (current_bytes, current_value, installed_value, removed_value) = match current {
            Ok((bytes, value)) => {
                match remove_owned(value.clone()).and_then(|removed| {
                    add_owned(removed.clone(), &owned_configuration)
                        .map(|installed| (removed, installed))
                }) {
                    Ok((removed, installed)) => {
                        (bytes, Some(value), Some(installed), Some(removed))
                    }
                    Err(error) => {
                        capability = unavailable_capability(
                            request.tool,
                            "configuration_malformed",
                            error.to_string(),
                        );
                        (bytes, None, None, None)
                    }
                }
            }
            Err(_) => (None, None, None, None),
        };
        Ok(Self {
            tool: request.tool,
            configuration_path,
            current_bytes,
            current_value,
            installed_value,
            removed_value,
            owned_configuration,
            workflow_contract_path,
            workflow_current,
            workflow_installed,
            compatibility_path,
            compatibility_current,
            compatibility_installed,
            capability,
        })
    }

    pub fn preview(
        &self,
        registration: Option<&IntegrationRegistration>,
        observations: &IntegrationObservations,
        operation: Option<IntegrationOperation>,
    ) -> Result<PreparedIntegrationPreview, AppError> {
        let status = self.status(registration, observations);
        let operation = match operation {
            Some(
                operation @ (IntegrationOperation::Install
                | IntegrationOperation::Repair
                | IntegrationOperation::Disable
                | IntegrationOperation::Remove),
            ) => operation,
            _ => IntegrationOperation::Install,
        };
        let value = self.value_for(operation);
        let contents = value.as_ref().map(encode_configuration).transpose()?;
        let workflow_contract_contents = self
            .workflow_value_for(operation)
            .map(|value| String::from_utf8_lossy(value).into_owned());
        let configuration_digest = configuration_digest(ConfigurationDigestInput {
            tool: self.tool,
            configuration_path: &self.configuration_path,
            current_contents: self.current_bytes.as_deref(),
            operation,
            proposed_contents: contents.as_deref(),
            workflow_path: &self.workflow_contract_path,
            current_workflow: self.workflow_current.as_deref(),
            proposed_workflow: self.workflow_value_for(operation),
            compatibility_path: &self.compatibility_path,
            current_compatibility: self.compatibility_current.as_deref(),
            proposed_compatibility: self.compatibility_value_for(operation),
        });
        Ok(PreparedIntegrationPreview {
            preview: IntegrationPreview {
                schema_version: 1,
                status,
                owned_configuration: self.owned_configuration.clone(),
                contents,
                workflow_contract_contents,
                confirmation_token: String::new(),
                will_change: self.current_value != *value
                    || self.workflow_current.as_deref() != self.workflow_value_for(operation)
                    || self.compatibility_current.as_deref()
                        != self.compatibility_value_for(operation),
            },
            confirmation_digest: configuration_digest,
        })
    }

    pub fn status(
        &self,
        registration: Option<&IntegrationRegistration>,
        observations: &IntegrationObservations,
    ) -> IntegrationStatus {
        let enabled = registration.is_some_and(|value| value.enabled);
        let state = if !self.capability.available {
            IntegrationState::Unavailable
        } else {
            let configuration_state =
                match (&self.current_value, &self.installed_value, registration) {
                    (Some(current), Some(installed), Some(value))
                        if value.enabled && current == installed =>
                    {
                        IntegrationState::Installed
                    }
                    (Some(current), Some(_), Some(value))
                        if !value.enabled && self.removed_value.as_ref() == Some(current) =>
                    {
                        IntegrationState::Disabled
                    }
                    (Some(current), Some(installed), None) if current == installed => {
                        IntegrationState::NeedsRepair
                    }
                    (Some(current), Some(installed), None)
                        if self.removed_value.as_ref() == Some(current) && current != installed =>
                    {
                        IntegrationState::NotInstalled
                    }
                    (Some(_), Some(_), _) => IntegrationState::NeedsRepair,
                    _ => IntegrationState::Unavailable,
                };
            match configuration_state {
                IntegrationState::Installed
                    if self.workflow_current.as_deref()
                        != Some(self.workflow_installed.as_slice())
                        || self.compatibility_current.as_deref()
                            != Some(self.compatibility_installed.as_slice()) =>
                {
                    IntegrationState::NeedsRepair
                }
                IntegrationState::Disabled | IntegrationState::NotInstalled
                    if self.workflow_current.is_some() || self.compatibility_current.is_some() =>
                {
                    IntegrationState::NeedsRepair
                }
                value => value,
            }
        };
        let available_operations = match state {
            IntegrationState::NotInstalled => vec![IntegrationOperation::Install],
            IntegrationState::Installed => {
                vec![IntegrationOperation::Disable, IntegrationOperation::Remove]
            }
            IntegrationState::NeedsRepair => {
                vec![IntegrationOperation::Repair, IntegrationOperation::Remove]
            }
            IntegrationState::Disabled => {
                vec![IntegrationOperation::Install, IntegrationOperation::Remove]
            }
            IntegrationState::Unavailable
                if self.removed_value.is_some()
                    && (registration.is_some() || self.current_value != self.removed_value) =>
            {
                vec![IntegrationOperation::Remove]
            }
            IntegrationState::Unavailable => Vec::new(),
        };
        IntegrationStatus {
            schema_version: 1,
            tool: self.tool,
            configuration_path: self.configuration_path.clone(),
            workflow_contract_path: self.workflow_contract_path.clone(),
            workflow_contract_version: WORKFLOW_CONTRACT_VERSION.to_owned(),
            state,
            enabled_in_workboard: enabled,
            adapter_version: registration.map(|value| value.adapter_version.clone()),
            first_observed_at: observations
                .first_observed_at
                .clone()
                .or_else(|| registration.and_then(|value| value.first_observed_at.clone())),
            last_observed_at: observations
                .last_observed_at
                .clone()
                .or_else(|| registration.and_then(|value| value.last_observed_at.clone())),
            last_hook_observed_at: observations.last_hook_observed_at.clone(),
            last_app_server_observed_at: observations.last_app_server_observed_at.clone(),
            capability: self.capability.clone(),
            available_operations,
        }
    }

    pub fn apply(
        &self,
        operation: IntegrationOperation,
    ) -> Result<ConfigurationMutation, AppError> {
        if matches!(
            operation,
            IntegrationOperation::Status | IntegrationOperation::Preview
        ) {
            return Ok(ConfigurationMutation {
                changed: false,
                backup_path: None,
                workflow_backup_path: None,
            });
        }
        if matches!(
            operation,
            IntegrationOperation::Install | IntegrationOperation::Repair
        ) && !self.capability.available
        {
            return Err(AppError::IntegrationUnavailable {
                tool: tool_name(self.tool),
                reason: self.capability.message.clone(),
            });
        }
        let workflow_mutation = self.apply_workflow(operation)?;
        let configuration_mutation = match self.apply_configuration(operation) {
            Ok(mutation) => mutation,
            Err(error) => {
                if workflow_mutation.changed {
                    self.rollback_workflow(operation)?;
                }
                return Err(error);
            }
        };
        Ok(ConfigurationMutation {
            changed: workflow_mutation.changed || configuration_mutation.changed,
            backup_path: configuration_mutation.backup_path,
            workflow_backup_path: workflow_mutation.backup_path,
        })
    }

    fn apply_configuration(
        &self,
        operation: IntegrationOperation,
    ) -> Result<ConfigurationMutation, AppError> {
        let proposed = match operation {
            IntegrationOperation::Install | IntegrationOperation::Repair => {
                if !self.capability.available {
                    return Err(AppError::IntegrationUnavailable {
                        tool: tool_name(self.tool),
                        reason: self.capability.message.clone(),
                    });
                }
                self.installed_value.as_ref()
            }
            IntegrationOperation::Disable | IntegrationOperation::Remove => {
                self.removed_value.as_ref()
            }
            IntegrationOperation::Status | IntegrationOperation::Preview => {
                return Ok(ConfigurationMutation {
                    changed: false,
                    backup_path: None,
                    workflow_backup_path: None,
                });
            }
        }
        .ok_or_else(|| AppError::IntegrationConfigurationMalformed {
            path: self.configuration_path.clone(),
            message: self.capability.message.clone(),
        })?;
        if self.current_value.as_ref() == Some(proposed) {
            return Ok(ConfigurationMutation {
                changed: false,
                backup_path: None,
                workflow_backup_path: None,
            });
        }
        let encoded = encode_configuration(proposed)?;
        replace_configuration(
            &self.configuration_path,
            self.current_bytes.as_deref(),
            encoded.as_bytes(),
        )
    }

    fn apply_workflow(
        &self,
        operation: IntegrationOperation,
    ) -> Result<ConfigurationMutation, AppError> {
        let workflow_target = self.workflow_value_for(operation);
        let workflow = mutate_owned_file(
            &self.workflow_contract_path,
            self.workflow_current.as_deref(),
            workflow_target,
        )?;
        let compatibility_target = self.compatibility_value_for(operation);
        let compatibility = match mutate_owned_file(
            &self.compatibility_path,
            self.compatibility_current.as_deref(),
            compatibility_target,
        ) {
            Ok(mutation) => mutation,
            Err(error) => {
                if workflow.changed {
                    rollback_owned_file(
                        &self.workflow_contract_path,
                        self.workflow_current.as_deref(),
                        workflow_target,
                    )?;
                }
                return Err(error);
            }
        };
        Ok(ConfigurationMutation {
            changed: workflow.changed || compatibility.changed,
            backup_path: workflow.backup_path.or(compatibility.backup_path),
            workflow_backup_path: None,
        })
    }

    fn rollback_workflow(&self, operation: IntegrationOperation) -> Result<(), AppError> {
        rollback_owned_file(
            &self.compatibility_path,
            self.compatibility_current.as_deref(),
            self.compatibility_value_for(operation),
        )?;
        rollback_owned_file(
            &self.workflow_contract_path,
            self.workflow_current.as_deref(),
            self.workflow_value_for(operation),
        )
    }

    pub fn confirmation_digest(&self, operation: IntegrationOperation) -> Result<String, AppError> {
        let contents = self
            .value_for(operation)
            .as_ref()
            .map(encode_configuration)
            .transpose()?;
        Ok(configuration_digest(ConfigurationDigestInput {
            tool: self.tool,
            configuration_path: &self.configuration_path,
            current_contents: self.current_bytes.as_deref(),
            operation,
            proposed_contents: contents.as_deref(),
            workflow_path: &self.workflow_contract_path,
            current_workflow: self.workflow_current.as_deref(),
            proposed_workflow: self.workflow_value_for(operation),
            compatibility_path: &self.compatibility_path,
            current_compatibility: self.compatibility_current.as_deref(),
            proposed_compatibility: self.compatibility_value_for(operation),
        }))
    }

    pub fn configuration_path(&self) -> &Path {
        &self.configuration_path
    }

    fn value_for(&self, operation: IntegrationOperation) -> &Option<Value> {
        match operation {
            IntegrationOperation::Disable | IntegrationOperation::Remove => &self.removed_value,
            _ => &self.installed_value,
        }
    }

    fn workflow_value_for(&self, operation: IntegrationOperation) -> Option<&[u8]> {
        match operation {
            IntegrationOperation::Install | IntegrationOperation::Repair => {
                Some(&self.workflow_installed)
            }
            IntegrationOperation::Disable | IntegrationOperation::Remove => None,
            IntegrationOperation::Status | IntegrationOperation::Preview => {
                Some(&self.workflow_installed)
            }
        }
    }

    fn compatibility_value_for(&self, operation: IntegrationOperation) -> Option<&[u8]> {
        match operation {
            IntegrationOperation::Install | IntegrationOperation::Repair => {
                Some(&self.compatibility_installed)
            }
            IntegrationOperation::Disable | IntegrationOperation::Remove => None,
            IntegrationOperation::Status | IntegrationOperation::Preview => {
                Some(&self.compatibility_installed)
            }
        }
    }
}

struct ConfigurationDigestInput<'a> {
    tool: Tool,
    configuration_path: &'a Path,
    current_contents: Option<&'a [u8]>,
    operation: IntegrationOperation,
    proposed_contents: Option<&'a str>,
    workflow_path: &'a Path,
    current_workflow: Option<&'a [u8]>,
    proposed_workflow: Option<&'a [u8]>,
    compatibility_path: &'a Path,
    current_compatibility: Option<&'a [u8]>,
    proposed_compatibility: Option<&'a [u8]>,
}

fn configuration_digest(input: ConfigurationDigestInput<'_>) -> String {
    let ConfigurationDigestInput {
        tool,
        configuration_path,
        current_contents,
        operation,
        proposed_contents,
        workflow_path,
        current_workflow,
        proposed_workflow,
        compatibility_path,
        current_compatibility,
        proposed_compatibility,
    } = input;
    let mut digest = Sha256::new();
    digest.update(b"agent-workboard/integration-confirmation/v1");
    digest_field(&mut digest, tool_name(tool).as_bytes());
    digest_field(&mut digest, configuration_path.to_string_lossy().as_bytes());
    digest.update([match operation {
        IntegrationOperation::Install => 1,
        IntegrationOperation::Repair => 2,
        IntegrationOperation::Disable => 3,
        IntegrationOperation::Remove => 4,
        IntegrationOperation::Status => 5,
        IntegrationOperation::Preview => 6,
    }]);
    match current_contents {
        Some(contents) => {
            digest.update([1]);
            digest_field(&mut digest, contents);
        }
        None => digest.update([0]),
    }
    match proposed_contents {
        Some(contents) => {
            digest.update([1]);
            digest_field(&mut digest, contents.as_bytes());
        }
        None => digest.update([0]),
    }
    digest_field(&mut digest, workflow_path.to_string_lossy().as_bytes());
    for value in [current_workflow, proposed_workflow] {
        match value {
            Some(contents) => {
                digest.update([1]);
                digest_field(&mut digest, contents);
            }
            None => digest.update([0]),
        }
    }
    digest_field(&mut digest, compatibility_path.to_string_lossy().as_bytes());
    for value in [current_compatibility, proposed_compatibility] {
        match value {
            Some(contents) => {
                digest.update([1]);
                digest_field(&mut digest, contents);
            }
            None => digest.update([0]),
        }
    }
    format!("{:x}", digest.finalize())
}

fn digest_field(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_le_bytes());
    digest.update(value);
}

pub struct ConfigurationMutation {
    pub changed: bool,
    pub backup_path: Option<PathBuf>,
    pub workflow_backup_path: Option<PathBuf>,
}

fn capability(
    tool: Tool,
    native_home: &Path,
    configuration: Result<&(Option<Vec<u8>>, Value), &AppError>,
) -> IntegrationCapability {
    let contract_version = contract_version(tool);
    let requires_native_review = tool == Tool::Codex;
    let unavailable = |code: &str, message: String| IntegrationCapability {
        available: false,
        code: code.to_owned(),
        message,
        contract_version: contract_version.clone(),
        requires_native_review,
    };
    let value = match configuration {
        Ok((_, value)) => value,
        Err(error) => return unavailable("configuration_malformed", error.to_string()),
    };
    if tool == Tool::Claude
        && value
            .get("disableAllHooks")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return unavailable(
            "hooks_disabled",
            "Claude hooks are disabled by disableAllHooks".to_owned(),
        );
    }
    if tool == Tool::Codex {
        for (path, managed) in [
            (native_home.join("config.toml"), false),
            (native_home.join("requirements.toml"), true),
        ] {
            match codex_hook_policy(&path, managed) {
                Ok(Some((code, message))) => return unavailable(code, message),
                Ok(None) => {}
                Err(error) => return unavailable("policy_malformed", error.to_string()),
            }
        }
    }
    IntegrationCapability {
        available: true,
        code: if requires_native_review {
            "available_review_required"
        } else {
            "available"
        }
        .to_owned(),
        message: if requires_native_review {
            "Codex must review and trust the installed user hook before it runs"
        } else {
            "Supported user-level lifecycle hooks are available"
        }
        .to_owned(),
        contract_version,
        requires_native_review,
    }
}

fn unavailable_capability(tool: Tool, code: &str, message: String) -> IntegrationCapability {
    IntegrationCapability {
        available: false,
        code: code.to_owned(),
        message,
        contract_version: contract_version(tool),
        requires_native_review: tool == Tool::Codex,
    }
}

fn contract_version(tool: Tool) -> String {
    match tool {
        Tool::Claude => "claude-settings-hooks-2026-08",
        Tool::Codex => "codex-hooks-json-2026-08",
    }
    .to_owned()
}

fn codex_hook_policy(
    path: &Path,
    managed: bool,
) -> Result<Option<(&'static str, String)>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = read_bounded(path)?;
    let text = std::str::from_utf8(&bytes).map_err(|error| {
        AppError::IntegrationConfigurationMalformed {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    let value: toml::Value =
        toml::from_str(text).map_err(|error| AppError::IntegrationConfigurationMalformed {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    let features = value.get("features").and_then(toml::Value::as_table);
    let hooks_disabled = features
        .and_then(|features| {
            features
                .get("hooks")
                .or_else(|| features.get("codex_hooks"))
        })
        .and_then(toml::Value::as_bool)
        == Some(false);
    if hooks_disabled {
        return Ok(Some((
            "hooks_disabled",
            format!("Codex hooks are disabled by {}", path.display()),
        )));
    }
    if managed
        && value
            .get("allow_managed_hooks_only")
            .and_then(toml::Value::as_bool)
            == Some(true)
    {
        return Ok(Some((
            "managed_hooks_only",
            format!(
                "Codex is configured to ignore user hooks in {}",
                path.display()
            ),
        )));
    }
    Ok(None)
}

fn read_json_configuration(path: &Path) -> Result<(Option<Vec<u8>>, Value), AppError> {
    if !path.exists() {
        return Ok((None, Value::Object(Map::new())));
    }
    let bytes = read_bounded(path)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::IntegrationConfigurationMalformed {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    if !value.is_object() {
        return Err(AppError::IntegrationConfigurationMalformed {
            path: path.to_path_buf(),
            message: "configuration root must be a JSON object".to_owned(),
        });
    }
    Ok((Some(bytes), value))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, AppError> {
    let metadata = fs::metadata(path).map_err(|source| AppError::IntegrationIo {
        operation: "reading configuration metadata",
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > MAX_CONFIGURATION_BYTES {
        return Err(AppError::IntegrationConfigurationTooLarge {
            path: path.to_path_buf(),
            limit: MAX_CONFIGURATION_BYTES,
        });
    }
    fs::read(path).map_err(|source| AppError::IntegrationIo {
        operation: "reading configuration",
        path: path.to_path_buf(),
        source,
    })
}

fn owned_configuration(tool: Tool, executable: &Path, database: &Path) -> Result<Value, AppError> {
    let executable = path_text(executable)?;
    let database = path_text(database)?;
    let events = match tool {
        Tool::Claude => [
            ("SessionStart", Some("startup|resume|clear|compact")),
            ("UserPromptSubmit", None),
            ("PreToolUse", Some("Bash")),
            ("CwdChanged", None),
            ("PreCompact", None),
            ("PostCompact", None),
            ("Stop", None),
            ("SessionEnd", None),
        ]
        .as_slice(),
        Tool::Codex => [
            ("SessionStart", Some("^(startup|resume|clear|compact)$")),
            ("UserPromptSubmit", None),
            ("PreToolUse", Some("^Bash$")),
            ("PreCompact", None),
            ("PostCompact", None),
            ("Stop", None),
            ("SessionEnd", None),
        ]
        .as_slice(),
    };
    let handler = match tool {
        Tool::Claude => json!({
            "type": "command",
            "command": executable,
            "args": hook_arguments("claude", database),
            "timeout": 3
        }),
        Tool::Codex => {
            let arguments = hook_arguments("codex", database);
            let command = shell_command(executable, &arguments, false);
            let command_windows = shell_command(executable, &arguments, true);
            json!({
                "type": "command",
                "command": command,
                "commandWindows": command_windows,
                "timeout": 3
            })
        }
    };
    let mut hooks = Map::new();
    for (event, matcher) in events {
        let mut group = Map::new();
        if let Some(matcher) = matcher {
            group.insert("matcher".to_owned(), Value::String((*matcher).to_owned()));
        }
        group.insert("hooks".to_owned(), Value::Array(vec![handler.clone()]));
        hooks.insert(
            (*event).to_owned(),
            Value::Array(vec![Value::Object(group)]),
        );
    }
    Ok(json!({ "hooks": hooks }))
}

fn hook_arguments(tool: &str, database: &str) -> Vec<String> {
    vec![
        "integration".to_owned(),
        "ingest-hook".to_owned(),
        "--tool".to_owned(),
        tool.to_owned(),
        "--database".to_owned(),
        database.to_owned(),
        "--owner".to_owned(),
        INTEGRATION_OWNER.to_owned(),
        "--quiet".to_owned(),
    ]
}

fn shell_command(executable: &str, arguments: &[String], windows: bool) -> String {
    std::iter::once(executable)
        .chain(arguments.iter().map(String::as_str))
        .map(|value| {
            if windows {
                format!("\"{}\"", value.replace('"', "\"\""))
            } else {
                format!("'{}'", value.replace('\'', "'\"'\"'"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn remove_owned(mut value: Value) -> Result<Value, AppError> {
    let root =
        value
            .as_object_mut()
            .ok_or_else(|| AppError::IntegrationConfigurationMalformed {
                path: PathBuf::new(),
                message: "configuration root must be a JSON object".to_owned(),
            })?;
    let Some(hooks) = root.get_mut("hooks") else {
        return Ok(value);
    };
    let hooks =
        hooks
            .as_object_mut()
            .ok_or_else(|| AppError::IntegrationConfigurationMalformed {
                path: PathBuf::new(),
                message: "hooks must be a JSON object".to_owned(),
            })?;
    hooks.retain(|_, groups| {
        let Some(groups) = groups.as_array_mut() else {
            return true;
        };
        groups.retain_mut(|group| {
            let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            handlers.retain(|handler| !is_owned_handler(handler));
            !handlers.is_empty()
        });
        !groups.is_empty()
    });
    if hooks.is_empty() {
        root.remove("hooks");
    }
    Ok(value)
}

fn add_owned(mut value: Value, owned: &Value) -> Result<Value, AppError> {
    let root =
        value
            .as_object_mut()
            .ok_or_else(|| AppError::IntegrationConfigurationMalformed {
                path: PathBuf::new(),
                message: "configuration root must be a JSON object".to_owned(),
            })?;
    if !root.contains_key("hooks") {
        root.insert("hooks".to_owned(), Value::Object(Map::new()));
    }
    let hooks = root
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AppError::IntegrationConfigurationMalformed {
            path: PathBuf::new(),
            message: "hooks must be a JSON object".to_owned(),
        })?;
    let owned_hooks = owned
        .get("hooks")
        .and_then(Value::as_object)
        .expect("owned configuration has hooks");
    for (event, groups) in owned_hooks {
        let target = hooks
            .entry(event.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        let target =
            target
                .as_array_mut()
                .ok_or_else(|| AppError::IntegrationConfigurationMalformed {
                    path: PathBuf::new(),
                    message: format!("hooks.{event} must be a JSON array"),
                })?;
        target.extend(
            groups
                .as_array()
                .expect("owned hook event is an array")
                .clone(),
        );
    }
    Ok(value)
}

fn is_owned_handler(value: &Value) -> bool {
    value
        .get("args")
        .and_then(Value::as_array)
        .is_some_and(|args| {
            args.windows(2).any(|pair| {
                pair[0].as_str() == Some("--owner") && pair[1].as_str() == Some(INTEGRATION_OWNER)
            })
        })
        || value
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|command| command.contains(INTEGRATION_OWNER))
        || value
            .get("commandWindows")
            .and_then(Value::as_str)
            .is_some_and(|command| command.contains(INTEGRATION_OWNER))
}

fn is_owned_workflow_contract(contents: &[u8]) -> bool {
    std::str::from_utf8(contents).is_ok_and(|contents| {
        contents.contains(&format!("owner: {INTEGRATION_OWNER}"))
            && (contents.contains("name: agent-workboard")
                || contents.contains("name: continue-roadmap"))
    })
}

fn mutate_owned_file(
    path: &Path,
    current: Option<&[u8]>,
    target: Option<&[u8]>,
) -> Result<ConfigurationMutation, AppError> {
    if current == target {
        return Ok(ConfigurationMutation {
            changed: false,
            backup_path: None,
            workflow_backup_path: None,
        });
    }
    match target {
        Some(contents) => replace_owned_file(path, current, contents),
        None if current.is_some_and(is_owned_workflow_contract) => {
            remove_owned_file(path, current.expect("owned workflow file"))
        }
        None => Ok(ConfigurationMutation {
            changed: false,
            backup_path: None,
            workflow_backup_path: None,
        }),
    }
}

fn rollback_owned_file(
    path: &Path,
    original: Option<&[u8]>,
    target: Option<&[u8]>,
) -> Result<(), AppError> {
    match original {
        Some(contents) => {
            replace_owned_file(path, target, contents)?;
        }
        None => {
            if let Some(contents) = target {
                remove_owned_file(path, contents)?;
            }
        }
    }
    Ok(())
}

fn replace_owned_file(
    path: &Path,
    expected: Option<&[u8]>,
    contents: &[u8],
) -> Result<ConfigurationMutation, AppError> {
    if !is_owned_workflow_contract(contents) {
        return Err(AppError::IntegrationConfigurationMalformed {
            path: path.to_path_buf(),
            message: "generated workflow contract has no ownership marker".to_owned(),
        });
    }
    if expected.is_some_and(|value| !is_owned_workflow_contract(value)) {
        return Err(AppError::IntegrationConfigurationChanged(
            path.to_path_buf(),
        ));
    }
    let parent = path.parent().ok_or_else(|| AppError::IntegrationIo {
        operation: "resolving workflow integration directory",
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing parent directory"),
    })?;
    fs::create_dir_all(parent).map_err(|source| AppError::IntegrationIo {
        operation: "creating workflow integration directory",
        path: parent.to_path_buf(),
        source,
    })?;
    let backup_path = expected
        .map(|bytes| create_backup(path, bytes))
        .transpose()?;
    let mut staged = NamedTempFile::new_in(parent).map_err(|source| AppError::IntegrationIo {
        operation: "staging workflow integration",
        path: path.to_path_buf(),
        source,
    })?;
    staged
        .write_all(contents)
        .and_then(|_| staged.as_file_mut().sync_all())
        .map_err(|source| AppError::IntegrationIo {
            operation: "writing staged workflow integration",
            path: path.to_path_buf(),
            source,
        })?;
    let actual = path.is_file().then(|| read_bounded(path)).transpose()?;
    if actual.as_deref() != expected {
        return Err(AppError::IntegrationConfigurationChanged(
            path.to_path_buf(),
        ));
    }
    let displaced = expected.map(|_| displace_configuration(path)).transpose()?;
    if let Some(displaced) = displaced.as_ref()
        && read_bounded(displaced)?.as_slice() != expected.expect("displaced workflow file")
    {
        restore_displaced_configuration(path, displaced)?;
        return Err(AppError::IntegrationConfigurationChanged(
            path.to_path_buf(),
        ));
    }
    if let Err(error) = staged.persist_noclobber(path) {
        if let Some(displaced) = displaced.as_ref() {
            restore_displaced_configuration(path, displaced)?;
        }
        return Err(if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            AppError::IntegrationConfigurationChanged(path.to_path_buf())
        } else {
            AppError::IntegrationIo {
                operation: "publishing workflow integration",
                path: path.to_path_buf(),
                source: error.error,
            }
        });
    }
    if let Some(displaced) = displaced {
        let _ = fs::remove_file(displaced);
    }
    Ok(ConfigurationMutation {
        changed: true,
        backup_path,
        workflow_backup_path: None,
    })
}

fn remove_owned_file(path: &Path, expected: &[u8]) -> Result<ConfigurationMutation, AppError> {
    if !is_owned_workflow_contract(expected) {
        return Err(AppError::IntegrationConfigurationChanged(
            path.to_path_buf(),
        ));
    }
    let backup_path = Some(create_backup(path, expected)?);
    let actual = read_bounded(path)?;
    if actual != expected {
        return Err(AppError::IntegrationConfigurationChanged(
            path.to_path_buf(),
        ));
    }
    let displaced = displace_configuration(path)?;
    if read_bounded(&displaced)? != expected {
        restore_displaced_configuration(path, &displaced)?;
        return Err(AppError::IntegrationConfigurationChanged(
            path.to_path_buf(),
        ));
    }
    fs::remove_file(&displaced).map_err(|source| AppError::IntegrationIo {
        operation: "removing workflow integration",
        path: path.to_path_buf(),
        source,
    })?;
    Ok(ConfigurationMutation {
        changed: true,
        backup_path,
        workflow_backup_path: None,
    })
}

fn replace_configuration(
    path: &Path,
    expected: Option<&[u8]>,
    contents: &[u8],
) -> Result<ConfigurationMutation, AppError> {
    replace_configuration_with(path, expected, contents, || Ok(()))
}

fn replace_configuration_with(
    path: &Path,
    expected: Option<&[u8]>,
    contents: &[u8],
    before_persist: impl FnOnce() -> Result<(), AppError>,
) -> Result<ConfigurationMutation, AppError> {
    let parent = path.parent().ok_or_else(|| AppError::IntegrationIo {
        operation: "resolving configuration directory",
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing parent directory"),
    })?;
    fs::create_dir_all(parent).map_err(|source| AppError::IntegrationIo {
        operation: "creating configuration directory",
        path: parent.to_path_buf(),
        source,
    })?;
    let backup_path = expected
        .map(|bytes| create_backup(path, bytes))
        .transpose()?;
    let mut staged = NamedTempFile::new_in(parent).map_err(|source| AppError::IntegrationIo {
        operation: "staging configuration",
        path: path.to_path_buf(),
        source,
    })?;
    staged
        .write_all(contents)
        .and_then(|_| staged.as_file_mut().sync_all())
        .map_err(|source| AppError::IntegrationIo {
            operation: "writing staged configuration",
            path: path.to_path_buf(),
            source,
        })?;
    serde_json::from_slice::<Value>(contents).map_err(|error| {
        AppError::IntegrationConfigurationMalformed {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    let actual = if path.exists() {
        Some(read_bounded(path)?)
    } else {
        None
    };
    if actual.as_deref() != expected {
        return Err(AppError::IntegrationConfigurationChanged(
            path.to_path_buf(),
        ));
    }
    if let Ok(metadata) = fs::metadata(path) {
        staged
            .as_file()
            .set_permissions(metadata.permissions())
            .map_err(|source| AppError::IntegrationIo {
                operation: "preserving configuration permissions",
                path: path.to_path_buf(),
                source,
            })?;
    }
    before_persist()?;
    let displaced = expected.map(|_| displace_configuration(path)).transpose()?;
    if let Some(displaced) = displaced.as_ref() {
        let replaced = match read_bounded(displaced) {
            Ok(replaced) => replaced,
            Err(error) => {
                restore_displaced_configuration(path, displaced)?;
                return Err(error);
            }
        };
        if Some(replaced.as_slice()) != expected {
            restore_displaced_configuration(path, displaced)?;
            return Err(AppError::IntegrationConfigurationChanged(
                path.to_path_buf(),
            ));
        }
    }
    if let Err(error) = staged.persist_noclobber(path) {
        if let Some(displaced) = displaced.as_ref() {
            if path.exists() {
                let _ = fs::remove_file(displaced);
            } else {
                restore_displaced_configuration(path, displaced)?;
            }
        }
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            return Err(AppError::IntegrationConfigurationChanged(
                path.to_path_buf(),
            ));
        }
        return Err(AppError::IntegrationIo {
            operation: "replacing configuration",
            path: path.to_path_buf(),
            source: error.error,
        });
    }
    if let Some(displaced) = displaced {
        let _ = fs::remove_file(displaced);
    }
    Ok(ConfigurationMutation {
        changed: true,
        backup_path,
        workflow_backup_path: None,
    })
}

fn displace_configuration(path: &Path) -> Result<PathBuf, AppError> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("configuration");
    let reservation = Builder::new()
        .prefix(&format!(".{file_name}.agent-workboard-displaced-"))
        .suffix(".bak")
        .tempfile_in(path.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|source| AppError::IntegrationIo {
            operation: "reserving displaced configuration",
            path: path.to_path_buf(),
            source,
        })?;
    let displaced = reservation.path().to_path_buf();
    drop(reservation);
    fs::rename(path, &displaced).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            AppError::IntegrationConfigurationChanged(path.to_path_buf())
        } else {
            AppError::IntegrationIo {
                operation: "capturing replaced configuration",
                path: path.to_path_buf(),
                source,
            }
        }
    })?;
    Ok(displaced)
}

fn restore_displaced_configuration(path: &Path, displaced: &Path) -> Result<(), AppError> {
    if path.exists() {
        return Ok(());
    }
    fs::rename(displaced, path).map_err(|source| AppError::IntegrationIo {
        operation: "restoring displaced configuration",
        path: path.to_path_buf(),
        source,
    })
}

fn create_backup(path: &Path, contents: &[u8]) -> Result<PathBuf, AppError> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("configuration");
    for index in 0..1000_u16 {
        let backup = path.with_file_name(format!("{file_name}.agent-workboard-backup-{index}.bak"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
        {
            Ok(mut file) => {
                file.write_all(contents)
                    .and_then(|_| file.sync_all())
                    .map_err(|source| AppError::IntegrationIo {
                        operation: "writing configuration backup",
                        path: backup.clone(),
                        source,
                    })?;
                return Ok(backup);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(AppError::IntegrationIo {
                    operation: "creating configuration backup",
                    path: backup,
                    source,
                });
            }
        }
    }
    Err(AppError::IntegrationIo {
        operation: "creating configuration backup",
        path: path.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "configuration backup namespace is exhausted",
        ),
    })
}

fn encode_configuration(value: &Value) -> Result<String, AppError> {
    let mut encoded = serde_json::to_string_pretty(value)?;
    if encoded.len() > MAX_PRETTY_CONFIGURATION_BYTES {
        encoded = serde_json::to_string(value)?;
    }
    encoded.push('\n');
    Ok(encoded)
}

fn configuration_path(tool: Tool, native_home: &Path) -> PathBuf {
    match tool {
        Tool::Claude => native_home.join("settings.json"),
        Tool::Codex => native_home.join("hooks.json"),
    }
}

fn canonical_file(path: &Path, label: &'static str) -> Result<PathBuf, AppError> {
    validate_absolute(path, label)?;
    let canonical = fs::canonicalize(path).map_err(|source| AppError::IntegrationIo {
        operation: "resolving integration input",
        path: path.to_path_buf(),
        source,
    })?;
    if !canonical.is_file() {
        return Err(AppError::IntegrationPathInvalid {
            label,
            path: path.to_path_buf(),
        });
    }
    Ok(native_compatible_path(&canonical))
}

#[cfg(windows)]
fn native_compatible_path(path: &Path) -> PathBuf {
    let value = path.as_os_str().to_string_lossy();
    if let Some(value) = value.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{value}"))
    } else if let Some(value) = value.strip_prefix(r"\\?\") {
        PathBuf::from(value)
    } else {
        path.to_owned()
    }
}

#[cfg(not(windows))]
fn native_compatible_path(path: &Path) -> PathBuf {
    path.to_owned()
}

fn validate_absolute(path: &Path, label: &'static str) -> Result<(), AppError> {
    if !path.is_absolute() {
        return Err(AppError::IntegrationPathNotAbsolute {
            label,
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<&str, AppError> {
    path.to_str()
        .filter(|value| !value.chars().any(char::is_control))
        .ok_or_else(|| AppError::IntegrationPathInvalid {
            label: "integration path",
            path: path.to_path_buf(),
        })
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "Claude",
        Tool::Codex => "Codex",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;
    use workboard_core::Tool;

    use super::{
        ADAPTER_VERSION, INTEGRATION_OWNER, IntegrationObservations, IntegrationOperation,
        IntegrationPlan, IntegrationRegistration, IntegrationRequest, IntegrationState,
        replace_configuration, replace_configuration_with,
    };

    #[test]
    fn claude_install_preserves_unrelated_settings_and_is_idempotent() {
        let fixture = Fixture::new(Tool::Claude);
        fs::write(
            fixture.home.join("settings.json"),
            r#"{"theme":"dark","hooks":{"Stop":[{"hooks":[{"type":"command","command":"other"}]}]}}"#,
        )
        .expect("write settings");
        let plan = fixture.plan(IntegrationOperation::Install);
        let first = plan
            .apply(IntegrationOperation::Install)
            .expect("install hooks");
        assert!(first.changed);
        assert!(first.backup_path.expect("backup").is_file());

        let installed: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.home.join("settings.json")).expect("read settings"),
        )
        .expect("parse settings");
        assert_eq!(installed["theme"], "dark");
        assert_eq!(
            installed["hooks"]["Stop"][0]["hooks"][0]["command"],
            "other"
        );
        assert!(installed.to_string().contains(INTEGRATION_OWNER));
        let workflow_path = fixture
            .home
            .join("skills")
            .join("agent-workboard")
            .join("SKILL.md");
        assert!(workflow_path.is_file());
        assert!(
            fs::read_to_string(&workflow_path)
                .expect("read workflow skill")
                .contains("feature_submit_proposal")
        );
        let compatibility_path = fixture
            .home
            .join("skills")
            .join("continue-roadmap")
            .join("SKILL.md");
        assert!(compatibility_path.is_file());
        assert!(
            fs::read_to_string(&compatibility_path)
                .expect("read compatibility skill")
                .contains("Do not plan in this unmanaged session")
        );

        let second = fixture
            .plan(IntegrationOperation::Install)
            .apply(IntegrationOperation::Install)
            .expect("repeat install");
        assert!(!second.changed);
        assert!(second.backup_path.is_none());
    }

    #[test]
    fn repair_replaces_only_owned_handlers_and_remove_preserves_unrelated_hooks() {
        let fixture = Fixture::new(Tool::Codex);
        fixture
            .plan(IntegrationOperation::Install)
            .apply(IntegrationOperation::Install)
            .expect("install hooks");
        let path = fixture.home.join("hooks.json");
        let mut installed: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("read hooks")).expect("parse hooks");
        installed["hooks"]["Stop"][0]["hooks"][0]["command"] = "broken".into();
        installed["hooks"]["Stop"][0]["hooks"]
            .as_array_mut()
            .expect("handlers")
            .push(serde_json::json!({"type":"command","command":"other"}));
        fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&installed).expect("encode hooks")
            ),
        )
        .expect("change hooks");

        fixture
            .plan(IntegrationOperation::Repair)
            .apply(IntegrationOperation::Repair)
            .expect("repair hooks");
        fixture
            .plan(IntegrationOperation::Remove)
            .apply(IntegrationOperation::Remove)
            .expect("remove hooks");
        let removed: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("read hooks")).expect("parse hooks");
        assert_eq!(removed["hooks"]["Stop"][0]["hooks"][0]["command"], "other");
        assert!(!removed.to_string().contains(INTEGRATION_OWNER));
        assert!(
            !fixture
                .home
                .join("skills")
                .join("agent-workboard")
                .join("SKILL.md")
                .exists()
        );
        assert!(
            !fixture
                .home
                .join("skills")
                .join("continue-roadmap")
                .join("SKILL.md")
                .exists()
        );
    }

    #[test]
    fn repairs_owned_workflow_drift_and_never_overwrites_a_foreign_skill() {
        let fixture = Fixture::new(Tool::Codex);
        fixture
            .plan(IntegrationOperation::Install)
            .apply(IntegrationOperation::Install)
            .expect("install integration");
        let workflow_path = fixture
            .home
            .join("skills")
            .join("agent-workboard")
            .join("SKILL.md");
        let mut drifted = fs::read_to_string(&workflow_path).expect("read workflow skill");
        drifted.push_str("\ndrift\n");
        fs::write(&workflow_path, drifted).expect("drift workflow skill");
        let repair = fixture.plan(IntegrationOperation::Repair);
        assert_eq!(
            repair
                .status(None, &IntegrationObservations::default())
                .state,
            IntegrationState::NeedsRepair
        );
        repair
            .apply(IntegrationOperation::Repair)
            .expect("repair workflow skill");
        assert!(
            !fs::read_to_string(&workflow_path)
                .expect("read repaired workflow skill")
                .contains("\ndrift\n")
        );

        let foreign = Fixture::new(Tool::Claude);
        let foreign_path = foreign
            .home
            .join("skills")
            .join("agent-workboard")
            .join("SKILL.md");
        fs::create_dir_all(foreign_path.parent().expect("skill directory"))
            .expect("create skill directory");
        fs::write(&foreign_path, "# Foreign skill\n").expect("write foreign skill");
        let install = foreign.plan(IntegrationOperation::Install);
        assert_eq!(
            install
                .status(None, &IntegrationObservations::default())
                .capability
                .code,
            "workflow_contract_conflict"
        );
        assert!(install.apply(IntegrationOperation::Install).is_err());
        assert_eq!(
            fs::read_to_string(&foreign_path).expect("read foreign skill"),
            "# Foreign skill\n"
        );
        assert!(!foreign.home.join("settings.json").exists());
    }

    #[test]
    fn reports_malformed_and_disabled_native_configuration_without_mutating_it() {
        let claude = Fixture::new(Tool::Claude);
        let path = claude.home.join("settings.json");
        fs::write(&path, "{").expect("write malformed settings");
        let before = fs::read(&path).expect("read settings");
        let plan = claude.plan(IntegrationOperation::Status);
        let observations = IntegrationObservations::default();
        assert_eq!(
            plan.status(None, &observations).state,
            IntegrationState::Unavailable
        );
        assert!(!plan.status(None, &observations).capability.available);
        assert_eq!(fs::read(&path).expect("read settings"), before);

        let codex = Fixture::new(Tool::Codex);
        fs::write(
            codex.home.join("config.toml"),
            "[features]\nhooks = false\n",
        )
        .expect("write config");
        let plan = codex.plan(IntegrationOperation::Preview);
        assert_eq!(
            plan.status(None, &observations).capability.code,
            "hooks_disabled"
        );
        assert!(plan.apply(IntegrationOperation::Install).is_err());
        assert!(!codex.home.join("hooks.json").exists());
    }

    #[test]
    fn unavailable_policy_keeps_owned_configuration_removable() {
        let fixture = Fixture::new(Tool::Codex);
        fixture
            .plan(IntegrationOperation::Install)
            .apply(IntegrationOperation::Install)
            .expect("install hooks");
        fs::write(
            fixture.home.join("config.toml"),
            "[features]\nhooks = false\n",
        )
        .expect("disable hooks");
        let plan = fixture.plan(IntegrationOperation::Status);
        let registration = IntegrationRegistration {
            enabled: true,
            adapter_version: ADAPTER_VERSION.to_owned(),
            first_observed_at: None,
            last_observed_at: None,
        };

        let status = plan.status(Some(&registration), &IntegrationObservations::default());

        assert_eq!(status.state, IntegrationState::Unavailable);
        assert_eq!(
            status.available_operations,
            vec![IntegrationOperation::Remove]
        );
    }

    #[test]
    fn stale_replacement_is_rejected_and_preserves_the_concurrent_change() {
        let directory = TempDir::new().expect("temp directory");
        let path = directory.path().join("settings.json");
        fs::write(&path, b"{}\n").expect("write settings");
        let expected = fs::read(&path).expect("read settings");
        fs::write(&path, b"{\"changed\":true}\n").expect("change settings");
        let result = replace_configuration(&path, Some(&expected), b"{\"installed\":true}\n");
        assert!(matches!(
            result,
            Err(crate::AppError::IntegrationConfigurationChanged(_))
        ));
        assert_eq!(
            fs::read(&path).expect("read settings"),
            b"{\"changed\":true}\n"
        );
    }

    #[test]
    fn final_replacement_race_restores_the_unreviewed_change() {
        let directory = TempDir::new().expect("temp directory");
        let path = directory.path().join("settings.json");
        let original = b"{}\n";
        let concurrent = b"{\"changed\":true}\n";
        fs::write(&path, original).expect("write settings");

        let result =
            replace_configuration_with(&path, Some(original), b"{\"installed\":true}\n", || {
                fs::write(&path, concurrent).map_err(|source| crate::AppError::IntegrationIo {
                    operation: "writing concurrent test configuration",
                    path: path.clone(),
                    source,
                })
            });

        assert!(matches!(
            result,
            Err(crate::AppError::IntegrationConfigurationChanged(_))
        ));
        assert_eq!(fs::read(path).expect("read settings"), concurrent);
    }

    #[test]
    fn interrupted_replacement_keeps_the_original_and_its_exact_backup() {
        let directory = TempDir::new().expect("temp directory");
        let path = directory.path().join("settings.json");
        let original = b"{ \"theme\" : \"dark\" }\n";
        fs::write(&path, original).expect("write settings");
        let result =
            replace_configuration_with(&path, Some(original), b"{\"installed\":true}\n", || {
                Err(crate::AppError::InjectedStorageInterruption)
            });
        assert!(matches!(
            result,
            Err(crate::AppError::InjectedStorageInterruption)
        ));
        assert_eq!(fs::read(&path).expect("read settings"), original);
        let backup = fs::read_dir(directory.path())
            .expect("list fixture")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|entry| {
                entry
                    .extension()
                    .is_some_and(|extension| extension == "bak")
            })
            .expect("backup path");
        assert_eq!(fs::read(backup).expect("read backup"), original);
    }

    struct Fixture {
        _directory: TempDir,
        home: std::path::PathBuf,
        executable: std::path::PathBuf,
        database: std::path::PathBuf,
        tool: Tool,
    }

    impl Fixture {
        fn new(tool: Tool) -> Self {
            let directory = TempDir::new().expect("temp directory");
            let home = directory.path().join(match tool {
                Tool::Claude => ".claude",
                Tool::Codex => ".codex",
            });
            fs::create_dir_all(&home).expect("create native home");
            let executable = directory.path().join("workboard.exe");
            let database = directory.path().join("workboard.sqlite");
            fs::write(&executable, b"fixture").expect("write executable");
            fs::write(&database, b"fixture").expect("write database");
            Self {
                _directory: directory,
                home,
                executable,
                database,
                tool,
            }
        }

        fn plan(&self, operation: IntegrationOperation) -> IntegrationPlan {
            IntegrationPlan::load(
                &IntegrationRequest {
                    tool: self.tool,
                    native_home: self.home.clone(),
                    workboard_executable: self.executable.clone(),
                    operation,
                    preview_operation: None,
                    confirmation: None,
                },
                &self.database,
            )
            .expect("load integration plan")
        }
    }
}
