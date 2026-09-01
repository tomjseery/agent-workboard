use std::collections::HashSet;
use std::io::{self, IsTerminal};
use std::path::Path;

use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use workboard_application::AppError;
use workboard_application::native_launch::{SystemLaunchExecutor, native_executable_available};
use workboard_application::recovery::{
    ProviderAvailability, RecordRecoveryOutcome, RecoveryDisposition, RecoveryEntry,
    RecoveryOutcomeStatus, RecoveryPreview,
};
use workboard_application::session_launch::BeginManagedSessionLaunch;
use workboard_application::workspace::WorkboardApplication;
use workboard_core::{ConversationId, ManagedLaunchMode, RecoveryAttemptId, Tool, WorkspaceId};

use crate::board;
use crate::selector::{SelectionCandidate, SelectionResult, resolve};
use crate::{
    RecoverArgs, await_binding, default_native_executable, default_terminal_executable,
    new_idempotency_key, output, tool_title,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryEntryResult {
    session_id: ConversationId,
    native_id: String,
    status: RecoveryOutcomeStatus,
    code: Option<String>,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryRun {
    attempt_id: RecoveryAttemptId,
    entries: Vec<RecoveryEntryResult>,
}

pub(crate) fn execute_recover(
    application: &mut WorkboardApplication,
    workspace_id: WorkspaceId,
    arguments: RecoverArgs,
    json: bool,
) -> Result<String, AppError> {
    let now = OffsetDateTime::now_utc();
    let since = arguments
        .since
        .as_deref()
        .map(|value| parse_since(value, now))
        .transpose()?;
    let terminal = arguments
        .terminal
        .unwrap_or_else(default_terminal_executable);
    let claude = arguments
        .claude
        .unwrap_or_else(|| default_native_executable(Tool::Claude));
    let codex = arguments
        .codex
        .unwrap_or_else(|| default_native_executable(Tool::Codex));
    let mut preview = application.recovery().preview(
        workspace_id,
        since,
        now,
        ProviderAvailability {
            claude: native_executable_available(&claude),
            codex: native_executable_available(&codex),
        },
    )?;
    let selected = resolve_selection(&preview, &arguments.sessions, json)?;
    preview
        .entries
        .retain(|entry| selected.contains(&entry.session_id));
    if arguments.dry_run {
        let human = format_preview(&preview);
        return output(&preview, json, human);
    }
    if preview.entries.is_empty() {
        return output(
            &preview,
            json,
            "No sessions are eligible for recovery".to_owned(),
        );
    }
    if arguments.sessions.is_empty() && !arguments.yes {
        if json || !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(AppError::External {
                code: "recovery_confirmation_required".to_owned(),
                message: "run interactively or pass --yes after reviewing --dry-run".to_owned(),
            });
        }
        let confirmed = board::checklist("Recover managed working set", candidates(&preview))?;
        let Some(confirmed) = confirmed else {
            return Ok("Recovery cancelled".to_owned());
        };
        preview
            .entries
            .retain(|entry| confirmed.contains(&entry.session_id.to_string()));
        if preview.entries.is_empty() {
            return Ok("Recovery cancelled: no sessions selected".to_owned());
        }
    }
    let selected = preview
        .entries
        .iter()
        .map(|entry| entry.session_id)
        .collect::<Vec<_>>();
    let idempotency_key = arguments
        .idempotency_key
        .unwrap_or_else(new_idempotency_key);
    let attempt_id =
        application
            .recovery()
            .begin_attempt(&preview, &selected, &idempotency_key, now)?;
    let mut results = Vec::with_capacity(preview.entries.len());
    for entry in &preview.entries {
        if let Some(prior) = application
            .recovery()
            .recorded_outcome(attempt_id, entry.session_id)?
        {
            match prior.status {
                RecoveryOutcomeStatus::Bound | RecoveryOutcomeStatus::Skipped => {
                    results.push(RecoveryEntryResult {
                        session_id: entry.session_id,
                        native_id: entry.native_id.clone(),
                        status: prior.status,
                        code: prior.code,
                        message: prior
                            .message
                            .unwrap_or_else(|| "reused durable recovery outcome".to_owned()),
                    });
                    continue;
                }
                RecoveryOutcomeStatus::Launched | RecoveryOutcomeStatus::Failed
                    if prior.launch_intent_id.is_some() =>
                {
                    results.push(RecoveryEntryResult {
                        session_id: entry.session_id,
                        native_id: entry.native_id.clone(),
                        status: RecoveryOutcomeStatus::Conflict,
                        code: Some("launch_reconciliation_required".to_owned()),
                        message: "an earlier launch crossed the process boundary and will not be duplicated"
                            .to_owned(),
                    });
                    continue;
                }
                _ => {}
            }
        }
        recover_entry(
            application,
            attempt_id,
            entry,
            &terminal,
            match entry.tool {
                Tool::Claude => &claude,
                Tool::Codex => &codex,
            },
            now,
            arguments.replace_unresumable,
            &mut results,
        )?;
    }
    application
        .recovery()
        .finish_attempt(attempt_id, OffsetDateTime::now_utc())?;
    let run = RecoveryRun {
        attempt_id,
        entries: results,
    };
    let restored = run
        .entries
        .iter()
        .filter(|entry| entry.status == RecoveryOutcomeStatus::Bound)
        .count();
    let conflicts = run
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.status,
                RecoveryOutcomeStatus::Conflict | RecoveryOutcomeStatus::Failed
            )
        })
        .count();
    output(
        &run,
        json,
        format!(
            "Recovery {attempt_id}: restored {restored}, skipped {}, conflicts {conflicts}",
            run.entries.len().saturating_sub(restored + conflicts)
        ),
    )
}

#[allow(clippy::too_many_arguments)]
fn recover_entry(
    application: &mut WorkboardApplication,
    attempt_id: RecoveryAttemptId,
    entry: &RecoveryEntry,
    terminal: &Path,
    native: &Path,
    now: OffsetDateTime,
    replace_unresumable: bool,
    results: &mut Vec<RecoveryEntryResult>,
) -> Result<(), AppError> {
    let replacing = matches!(
        &entry.disposition,
        RecoveryDisposition::Conflict { code, .. }
            if code == "unresumable" && replace_unresumable
    );
    match &entry.disposition {
        RecoveryDisposition::AlreadyLive => {
            return record(
                application,
                attempt_id,
                entry,
                RecoveryOutcomeStatus::Skipped,
                None,
                Some("already_live"),
                "a confirmed live process already owns this session",
                results,
            );
        }
        RecoveryDisposition::Conflict { code, message } if !replacing => {
            return record(
                application,
                attempt_id,
                entry,
                RecoveryOutcomeStatus::Conflict,
                None,
                Some(code),
                message,
                results,
            );
        }
        RecoveryDisposition::Conflict { .. } => {}
        RecoveryDisposition::ReadyRecreate => {
            if let Err(error) = application.recovery().recreate_checkout(entry, now) {
                let code = error.code().to_owned();
                return record(
                    application,
                    attempt_id,
                    entry,
                    RecoveryOutcomeStatus::Failed,
                    None,
                    Some(&code),
                    &error.to_string(),
                    results,
                );
            }
        }
        RecoveryDisposition::ReadyPresent => {}
    }
    if let Err(error) = application
        .checkout_service()
        .reconcile_registered_checkout(entry.checkout_id, now)
    {
        let code = error.code().to_owned();
        return record(
            application,
            attempt_id,
            entry,
            RecoveryOutcomeStatus::Failed,
            None,
            Some(&code),
            &error.to_string(),
            results,
        );
    }
    let context = if replacing {
        None
    } else {
        match application.native_sources().resume_context(
            entry.session_id,
            entry.checkout_path.clone(),
            entry.tab_title.clone(),
        ) {
            Ok(context) => Some(context),
            Err(error) => {
                let code = error.code().to_owned();
                return record(
                    application,
                    attempt_id,
                    entry,
                    RecoveryOutcomeStatus::Failed,
                    None,
                    Some(&code),
                    &error.to_string(),
                    results,
                );
            }
        }
    };
    let capability =
        match crate::capability_inputs(application, entry.tool, &entry.repository_id.to_string()) {
            Ok(capability) => capability,
            Err(error) => {
                let code = error.code().to_owned();
                return record(
                    application,
                    attempt_id,
                    entry,
                    RecoveryOutcomeStatus::Failed,
                    None,
                    Some(&code),
                    &error.to_string(),
                    results,
                );
            }
        };
    let prepared = match application
        .session_launch()
        .begin(BeginManagedSessionLaunch {
            owner: entry.owner,
            role: entry.role,
            tool: entry.tool,
            mode: if replacing {
                ManagedLaunchMode::New
            } else {
                ManagedLaunchMode::Resume(entry.native_id.clone())
            },
            checkout_id: entry.checkout_id,
            working_directory: entry.checkout_path.clone(),
            title: entry.tab_title.clone(),
            terminal_window: Some(entry.window_key.clone()),
            terminal_executable: terminal.to_path_buf(),
            native_executable: native.to_path_buf(),
            idempotency_key: format!(
                "recovery:{attempt_id}:{}{}",
                entry.session_id,
                if replacing { ":replacement" } else { "" }
            ),
            created_at: now,
            expires_at: now + time::Duration::minutes(2),
            resume_context: context,
            initial_prompt: replacing.then(|| {
                format!(
                    "Continue {} as a confirmed replacement for unresumable native session {}. Read the assigned Workboard hierarchy and preserve its existing history.",
                    entry.tab_title, entry.native_id
                )
            }),
            capability,
        }) {
        Ok(prepared) => prepared,
        Err(error) => {
            let code = error.code().to_owned();
            return record(
                application,
                attempt_id,
                entry,
                RecoveryOutcomeStatus::Failed,
                None,
                Some(&code),
                &error.to_string(),
                results,
            );
        }
    };
    if let Err(error) = application
        .session_launch()
        .execute(&prepared, &SystemLaunchExecutor)
    {
        let code = error.code().to_owned();
        return record(
            application,
            attempt_id,
            entry,
            RecoveryOutcomeStatus::Failed,
            Some(prepared.intent_id),
            Some(&code),
            &error.to_string(),
            results,
        );
    }
    application
        .recovery()
        .record_outcome(RecordRecoveryOutcome {
            attempt_id,
            session_id: entry.session_id,
            status: RecoveryOutcomeStatus::Launched,
            launch_intent_id: Some(prepared.intent_id),
            code: None,
            message: None,
            observed_at: OffsetDateTime::now_utc(),
        })?;
    match await_binding(application, prepared.intent_id) {
        Ok(_) => record(
            application,
            attempt_id,
            entry,
            RecoveryOutcomeStatus::Bound,
            Some(prepared.intent_id),
            None,
            &if replacing {
                format!("started replacement {} session", tool_title(entry.tool))
            } else {
                format!("restored exact {} session", tool_title(entry.tool))
            },
            results,
        ),
        Err(error) => {
            let code = error.code().to_owned();
            record(
                application,
                attempt_id,
                entry,
                RecoveryOutcomeStatus::Failed,
                Some(prepared.intent_id),
                Some(&code),
                &error.to_string(),
                results,
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn record(
    application: &mut WorkboardApplication,
    attempt_id: RecoveryAttemptId,
    entry: &RecoveryEntry,
    status: RecoveryOutcomeStatus,
    launch_intent_id: Option<workboard_core::LaunchIntentId>,
    code: Option<&str>,
    message: &str,
    results: &mut Vec<RecoveryEntryResult>,
) -> Result<(), AppError> {
    application
        .recovery()
        .record_outcome(RecordRecoveryOutcome {
            attempt_id,
            session_id: entry.session_id,
            status,
            launch_intent_id,
            code: code.map(str::to_owned),
            message: Some(message.to_owned()),
            observed_at: OffsetDateTime::now_utc(),
        })?;
    results.push(RecoveryEntryResult {
        session_id: entry.session_id,
        native_id: entry.native_id.clone(),
        status,
        code: code.map(str::to_owned),
        message: message.to_owned(),
    });
    Ok(())
}

fn resolve_selection(
    preview: &RecoveryPreview,
    queries: &[String],
    json: bool,
) -> Result<HashSet<ConversationId>, AppError> {
    if queries.is_empty() {
        return Ok(preview
            .entries
            .iter()
            .map(|entry| entry.session_id)
            .collect());
    }
    let candidates = candidates(preview);
    let mut selected = HashSet::new();
    for query in queries {
        let candidate = match resolve(Some(query), candidates.clone()) {
            SelectionResult::Selected(candidate) => Some(candidate),
            SelectionResult::Picker(matches) if !json && io::stdin().is_terminal() => board::pick(
                "Select recovery session",
                matches
                    .into_iter()
                    .map(|candidate| candidate.candidate)
                    .collect(),
            )?,
            SelectionResult::Picker(_) => {
                return Err(AppError::External {
                    code: "selection_ambiguous".to_owned(),
                    message: format!("recovery session query is ambiguous: {query}"),
                });
            }
            SelectionResult::Empty => None,
        };
        let Some(candidate) = candidate else {
            return Err(AppError::ConversationNotFound);
        };
        selected.insert(
            candidate
                .id
                .parse()
                .map_err(|error| AppError::Domain(format!("invalid session ID: {error}")))?,
        );
    }
    Ok(selected)
}

fn candidates(preview: &RecoveryPreview) -> Vec<SelectionCandidate> {
    preview
        .entries
        .iter()
        .map(|entry| SelectionCandidate {
            id: entry.session_id.to_string(),
            key: Some(entry.native_id.clone()),
            label: entry.tab_title.clone(),
            metadata: format!(
                "{}  {}",
                entry.window_title,
                disposition_name(&entry.disposition)
            ),
        })
        .collect()
}

fn format_preview(preview: &RecoveryPreview) -> String {
    let mut text = format!("Recovery plan: {} session(s)\n", preview.entries.len());
    let mut window = None;
    for entry in &preview.entries {
        if window.as_deref() != Some(entry.window_key.as_str()) {
            window = Some(entry.window_key.clone());
            text.push_str(&format!("\nWindow: {}\n", entry.window_title));
        }
        text.push_str(&format!(
            "  [{}] {} at {}\n",
            disposition_name(&entry.disposition),
            entry.tab_title,
            entry.checkout_path.display()
        ));
    }
    text
}

fn disposition_name(disposition: &RecoveryDisposition) -> &'static str {
    match disposition {
        RecoveryDisposition::ReadyPresent => "ready",
        RecoveryDisposition::ReadyRecreate => "recreate",
        RecoveryDisposition::AlreadyLive => "already live",
        RecoveryDisposition::Conflict { .. } => "conflict",
    }
}

fn parse_since(value: &str, now: OffsetDateTime) -> Result<OffsetDateTime, AppError> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("yesterday") {
        return Ok(now - time::Duration::days(1));
    }
    if let Some(days) = value
        .strip_suffix('d')
        .and_then(|days| days.parse::<i64>().ok())
    {
        return Ok(now - time::Duration::days(days));
    }
    if let Some(hours) = value
        .strip_suffix('h')
        .and_then(|hours| hours.parse::<i64>().ok())
    {
        return Ok(now - time::Duration::hours(hours));
    }
    OffsetDateTime::parse(value, &Rfc3339).map_err(|_| AppError::External {
        code: "invalid_recovery_period".to_owned(),
        message: "--since accepts yesterday, Nd, Nh, or an RFC 3339 timestamp".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::parse_since;

    fn at(value: &str) -> OffsetDateTime {
        OffsetDateTime::parse(value, &Rfc3339).unwrap()
    }

    #[test]
    fn parses_yesterday_durations_and_exact_timestamps() {
        let now = at("2026-08-28T12:00:00Z");
        assert_eq!(
            parse_since("yesterday", now).unwrap(),
            at("2026-08-27T12:00:00Z")
        );
        assert_eq!(parse_since("2d", now).unwrap(), at("2026-08-26T12:00:00Z"));
        assert_eq!(parse_since("6h", now).unwrap(), at("2026-08-28T06:00:00Z"));
        assert_eq!(
            parse_since("2026-08-01T00:00:00Z", now).unwrap(),
            at("2026-08-01T00:00:00Z")
        );
        assert!(parse_since("last Tuesday", now).is_err());
    }
}
