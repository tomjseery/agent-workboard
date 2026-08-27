use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;
use workboard_core::Tool;

use crate::AppError;
use crate::integration::{
    ADAPTER_VERSION, IntegrationObservations, IntegrationOperation, IntegrationPlan,
    IntegrationRegistration, IntegrationRequest, IntegrationResponse,
};
use crate::storage::SqliteStore;

const CONFIRMATION_TTL_SECONDS: i64 = 300;

pub(crate) fn record_hook_observation(
    transaction: &rusqlite::Transaction<'_>,
    tool: Tool,
    observed_at: OffsetDateTime,
) -> Result<(), AppError> {
    let observed_at = timestamp(observed_at);
    transaction.execute(
        "INSERT INTO integration_observations (
             provider, first_observed_at, last_observed_at, last_hook_observed_at
         ) VALUES (?1, ?2, ?2, ?2)
         ON CONFLICT(provider) DO UPDATE SET
             last_observed_at = excluded.last_observed_at,
             last_hook_observed_at = excluded.last_hook_observed_at",
        rusqlite::params![tool_name(tool), observed_at],
    )?;
    Ok(())
}

pub struct IntegrationService<'a> {
    store: &'a mut SqliteStore,
}

impl<'a> IntegrationService<'a> {
    pub fn new(store: &'a mut SqliteStore) -> Self {
        Self { store }
    }

    pub fn execute(
        &mut self,
        request: IntegrationRequest,
        now: OffsetDateTime,
    ) -> Result<IntegrationResponse, AppError> {
        let plan = IntegrationPlan::load(&request, self.store.path())?;
        let registration = self.registration(request.tool)?;
        let observations = self.observations(request.tool)?;
        match request.operation {
            IntegrationOperation::Status => Ok(IntegrationResponse::Status {
                status: plan.status(registration.as_ref(), &observations),
            }),
            IntegrationOperation::Preview => {
                let operation = mutation_operation(request.preview_operation);
                let prepared =
                    plan.preview(registration.as_ref(), &observations, Some(operation))?;
                let token = Uuid::new_v4().to_string();
                self.store_confirmation(
                    request.tool,
                    operation,
                    &token,
                    &prepared.confirmation_digest,
                    now,
                )?;
                Ok(IntegrationResponse::Preview {
                    preview: prepared.into_preview(token),
                })
            }
            operation => {
                let token = request
                    .confirmation
                    .as_ref()
                    .map(|confirmation| confirmation.token.as_str())
                    .ok_or_else(|| {
                        AppError::IntegrationConfirmationInvalid(self.store.path().to_path_buf())
                    })?;
                let digest = plan.confirmation_digest(operation)?;
                self.validate_confirmation(request.tool, operation, token, &digest, now)?;
                let mutation = plan.apply(operation)?;
                self.complete_mutation(request.tool, operation, token, now)?;
                let refreshed = IntegrationPlan::load(&request, self.store.path())?;
                let registration = self.registration(request.tool)?;
                let observations = self.observations(request.tool)?;
                Ok(IntegrationResponse::Mutation {
                    outcome: crate::integration::IntegrationOutcome {
                        schema_version: 1,
                        operation,
                        changed: mutation.changed,
                        backup_path: mutation.backup_path,
                        workflow_backup_path: mutation.workflow_backup_path,
                        status: refreshed.status(registration.as_ref(), &observations),
                    },
                })
            }
        }
    }

    fn registration(&self, tool: Tool) -> Result<Option<IntegrationRegistration>, AppError> {
        self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT enabled, adapter_version, first_observed_at, last_observed_at
                     FROM integration_registrations WHERE provider = ?1",
                    [tool_name(tool)],
                    |row| {
                        Ok(IntegrationRegistration {
                            enabled: row.get::<_, i64>(0)? != 0,
                            adapter_version: row.get(1)?,
                            first_observed_at: row.get(2)?,
                            last_observed_at: row.get(3)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
        })
    }

    fn observations(&self, tool: Tool) -> Result<IntegrationObservations, AppError> {
        self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT first_observed_at, last_observed_at,
                            last_hook_observed_at, last_app_server_observed_at
                     FROM integration_observations WHERE provider = ?1",
                    [tool_name(tool)],
                    |row| {
                        Ok(IntegrationObservations {
                            first_observed_at: row.get(0)?,
                            last_observed_at: row.get(1)?,
                            last_hook_observed_at: row.get(2)?,
                            last_app_server_observed_at: row.get(3)?,
                        })
                    },
                )
                .optional()?
                .map_or_else(|| Ok(IntegrationObservations::default()), Ok)
        })
    }

    fn store_confirmation(
        &mut self,
        tool: Tool,
        operation: IntegrationOperation,
        token: &str,
        digest: &str,
        now: OffsetDateTime,
    ) -> Result<(), AppError> {
        self.store.write(|transaction| {
            transaction.execute(
                "INSERT INTO integration_confirmations (
                     token_hash, provider, operation, configuration_digest,
                     created_at, expires_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    token_hash(token),
                    tool_name(tool),
                    operation_name(operation),
                    digest,
                    timestamp(now),
                    timestamp(now + time::Duration::seconds(CONFIRMATION_TTL_SECONDS)),
                ],
            )?;
            Ok(())
        })
    }

    fn validate_confirmation(
        &self,
        tool: Tool,
        operation: IntegrationOperation,
        token: &str,
        digest: &str,
        now: OffsetDateTime,
    ) -> Result<(), AppError> {
        let valid = self.store.read(|connection| {
            connection
                .query_row(
                    "SELECT EXISTS (
                     SELECT 1 FROM integration_confirmations
                     WHERE token_hash = ?1 AND provider = ?2 AND operation = ?3
                       AND configuration_digest = ?4 AND consumed_at IS NULL
                       AND expires_at > ?5
                 )",
                    rusqlite::params![
                        token_hash(token),
                        tool_name(tool),
                        operation_name(operation),
                        digest,
                        timestamp(now),
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(Into::into)
        })?;
        if valid != 0 {
            Ok(())
        } else {
            Err(AppError::IntegrationConfirmationInvalid(
                self.store.path().to_path_buf(),
            ))
        }
    }

    fn complete_mutation(
        &mut self,
        tool: Tool,
        operation: IntegrationOperation,
        token: &str,
        now: OffsetDateTime,
    ) -> Result<(), AppError> {
        let enabled = matches!(
            operation,
            IntegrationOperation::Install | IntegrationOperation::Repair
        );
        let now = timestamp(now);
        self.store.write(|transaction| {
            transaction.execute(
                "UPDATE integration_confirmations SET consumed_at = ?2
                 WHERE token_hash = ?1 AND consumed_at IS NULL",
                rusqlite::params![token_hash(token), now],
            )?;
            if operation == IntegrationOperation::Remove {
                transaction.execute(
                    "DELETE FROM integration_registrations WHERE provider = ?1",
                    [tool_name(tool)],
                )?;
            } else {
                transaction.execute(
                    "INSERT INTO integration_registrations (
                         provider, enabled, adapter_version, first_observed_at, last_observed_at
                     ) VALUES (?1, ?2, ?3, ?4, ?4)
                     ON CONFLICT(provider) DO UPDATE SET
                         enabled = excluded.enabled,
                         adapter_version = excluded.adapter_version,
                         last_observed_at = excluded.last_observed_at",
                    rusqlite::params![tool_name(tool), i64::from(enabled), ADAPTER_VERSION, now,],
                )?;
            }
            Ok(())
        })
    }
}

fn mutation_operation(operation: Option<IntegrationOperation>) -> IntegrationOperation {
    match operation {
        Some(
            operation @ (IntegrationOperation::Install
            | IntegrationOperation::Repair
            | IntegrationOperation::Disable
            | IntegrationOperation::Remove),
        ) => operation,
        _ => IntegrationOperation::Install,
    }
}

fn operation_name(operation: IntegrationOperation) -> &'static str {
    match operation {
        IntegrationOperation::Status => "status",
        IntegrationOperation::Preview => "preview",
        IntegrationOperation::Install => "install",
        IntegrationOperation::Repair => "repair",
        IntegrationOperation::Disable => "disable",
        IntegrationOperation::Remove => "remove",
    }
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "claude",
        Tool::Codex => "codex",
    }
}

fn timestamp(value: OffsetDateTime) -> String {
    value.unix_timestamp_nanos().to_string()
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;
    use time::OffsetDateTime;
    use workboard_core::Tool;

    use super::IntegrationService;
    use crate::AppError;
    use crate::integration::{
        IntegrationConfirmation, IntegrationOperation, IntegrationRequest, IntegrationResponse,
        IntegrationState,
    };
    use crate::storage::SqliteStore;

    struct Fixture {
        _directory: TempDir,
        store: SqliteStore,
        home: std::path::PathBuf,
        executable: std::path::PathBuf,
        observed_at: OffsetDateTime,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = TempDir::new().expect("temporary directory");
            let home = directory.path().join(".claude");
            fs::create_dir(&home).expect("native home");
            let executable = directory.path().join("workboard.exe");
            fs::write(&executable, []).expect("workboard executable");
            let store =
                SqliteStore::open(directory.path().join("workboard.sqlite")).expect("open store");
            Self {
                _directory: directory,
                store,
                home,
                executable,
                observed_at: OffsetDateTime::parse(
                    "2026-08-27T12:00:00Z",
                    &time::format_description::well_known::Rfc3339,
                )
                .expect("timestamp"),
            }
        }

        fn request(
            &self,
            operation: IntegrationOperation,
            preview_operation: Option<IntegrationOperation>,
            token: Option<String>,
        ) -> IntegrationRequest {
            IntegrationRequest {
                tool: Tool::Claude,
                native_home: self.home.clone(),
                workboard_executable: self.executable.clone(),
                operation,
                preview_operation,
                confirmation: token.map(|token| IntegrationConfirmation { token }),
            }
        }
    }

    #[test]
    fn preview_confirmation_is_required_once_and_install_is_registered() {
        let mut fixture = Fixture::new();
        let preview_request = fixture.request(
            IntegrationOperation::Preview,
            Some(IntegrationOperation::Install),
            None,
        );
        let preview = IntegrationService::new(&mut fixture.store)
            .execute(preview_request, fixture.observed_at)
            .expect("preview integration");
        let token = match preview {
            IntegrationResponse::Preview { preview } => preview.confirmation_token,
            _ => panic!("expected preview"),
        };
        let wrong = fixture.request(
            IntegrationOperation::Install,
            None,
            Some("wrong-token".to_owned()),
        );
        assert!(matches!(
            IntegrationService::new(&mut fixture.store).execute(wrong, fixture.observed_at),
            Err(AppError::IntegrationConfirmationInvalid(_))
        ));
        let install = fixture.request(IntegrationOperation::Install, None, Some(token.clone()));
        let installed = IntegrationService::new(&mut fixture.store)
            .execute(install, fixture.observed_at)
            .expect("install integration");
        assert!(matches!(
            installed,
            IntegrationResponse::Mutation { ref outcome }
                if outcome.status.state == IntegrationState::Installed
        ));
        assert!(fixture.home.join("settings.json").is_file());
        let repeated = fixture.request(IntegrationOperation::Install, None, Some(token));
        assert!(matches!(
            IntegrationService::new(&mut fixture.store).execute(repeated, fixture.observed_at),
            Err(AppError::IntegrationConfirmationInvalid(_))
        ));
    }

    #[test]
    fn expired_confirmation_cannot_mutate_native_configuration() {
        let mut fixture = Fixture::new();
        let preview_request = fixture.request(
            IntegrationOperation::Preview,
            Some(IntegrationOperation::Install),
            None,
        );
        let preview = IntegrationService::new(&mut fixture.store)
            .execute(preview_request, fixture.observed_at)
            .expect("preview integration");
        let token = match preview {
            IntegrationResponse::Preview { preview } => preview.confirmation_token,
            _ => panic!("expected preview"),
        };
        let install = fixture.request(IntegrationOperation::Install, None, Some(token));
        assert!(matches!(
            IntegrationService::new(&mut fixture.store)
                .execute(install, fixture.observed_at + time::Duration::minutes(6),),
            Err(AppError::IntegrationConfirmationInvalid(_))
        ));
        assert!(!fixture.home.join("settings.json").exists());
    }
}
