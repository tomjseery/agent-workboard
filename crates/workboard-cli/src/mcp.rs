use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use workboard_application::AppError;
use workboard_application::follow_up::{SendSessionFollowUp, SystemFollowUpExecutor};
use workboard_application::workflow_operations::CheckpointWorkItem;
use workboard_application::workspace::WorkboardApplication;
use workboard_core::HierarchyOwner;

use super::{
    FeatureProposalRequest, FeaturePublicationRequest, ManagedSessionRequest,
    SessionFollowUpRequest, WorkItemCheckpointRequest, execute_managed_session_request,
    workflow_token,
};

const MAX_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

pub fn run(database: PathBuf) -> Result<(), AppError> {
    let mut application = WorkboardApplication::open(database)?;
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut line = String::new();
    loop {
        line.clear();
        let read = input.read_line(&mut line).map_err(AppError::HookInputIo)?;
        if read == 0 {
            return Ok(());
        }
        if line.len() > MAX_MESSAGE_BYTES {
            write_response(
                &mut output,
                &error_response(Value::Null, -32600, "MCP message exceeds 2 MiB"),
            )?;
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(_) => {
                write_response(
                    &mut output,
                    &error_response(Value::Null, -32700, "invalid JSON"),
                )?;
                continue;
            }
        };
        if let Some(response) = handle(&mut application, &request) {
            write_response(&mut output, &response)?;
        }
    }
}

fn write_response(output: &mut impl Write, response: &Value) -> Result<(), AppError> {
    serde_json::to_writer(&mut *output, response)?;
    output.write_all(b"\n").map_err(AppError::HookInputIo)?;
    output.flush().map_err(AppError::HookInputIo)
}

fn handle(application: &mut WorkboardApplication, request: &Value) -> Option<Value> {
    let id = request.get("id")?.clone();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let result = match method {
        "initialize" => Ok(initialize_result(request)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(application, request),
        _ => return Some(error_response(id, -32601, "method not found")),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{
                    "type": "text",
                    "text": format!("{}: {}", error.code(), error)
                }],
                "isError": true
            }
        }),
    })
}

fn initialize_result(request: &Value) -> Value {
    let protocol_version = request
        .pointer("/params/protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or("2025-03-26");
    json!({
        "protocolVersion": protocol_version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "agent-workboard", "version": env!("CARGO_PKG_VERSION") }
    })
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "hierarchy_read",
            "description": "Read the versioned hierarchy, exact checkout, and repository instructions assigned to this managed session.",
            "inputSchema": { "type": "object", "additionalProperties": false }
        },
        {
            "name": "epic_propose",
            "description": "Submit a typed Epic proposal from a managed workspace-planning session for explicit user approval.",
            "inputSchema": {
                "type": "object",
                "required": ["title", "body", "idempotencyKey"],
                "properties": {
                    "title": { "type": "string", "minLength": 1 },
                    "slug": { "type": "string" },
                    "body": { "type": "string", "minLength": 1 },
                    "idempotencyKey": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "epic_propose_research",
            "description": "Submit imported or researched Markdown as a typed Epic proposal, recording every source it was read from.",
            "inputSchema": {
                "type": "object",
                "required": ["title", "body", "sources", "idempotencyKey"],
                "properties": {
                    "title": { "type": "string", "minLength": 1 },
                    "slug": { "type": "string" },
                    "body": { "type": "string", "minLength": 1 },
                    "sources": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "required": ["path", "contentHash"],
                            "properties": {
                                "path": { "type": "string", "minLength": 1 },
                                "contentHash": { "type": "string", "minLength": 1 }
                            },
                            "additionalProperties": false
                        }
                    },
                    "idempotencyKey": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "feature_propose",
            "description": "Submit a typed Feature proposal under an existing Epic for explicit user approval.",
            "inputSchema": {
                "type": "object",
                "required": ["epicId", "title", "outcome", "idempotencyKey"],
                "properties": {
                    "epicId": { "type": "string", "format": "uuid" },
                    "title": { "type": "string", "minLength": 1 },
                    "slug": { "type": "string" },
                    "outcome": { "type": "string", "minLength": 1 },
                    "idempotencyKey": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "feature_submit_proposal",
            "description": "Submit one complete Feature and Work-item proposal for explicit user approval.",
            "inputSchema": {
                "type": "object",
                "required": ["featureId", "idempotencyKey", "proposal"],
                "properties": {
                    "featureId": { "type": "string", "format": "uuid" },
                    "idempotencyKey": { "type": "string", "minLength": 1 },
                    "proposal": { "type": "object" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "feature_publish",
            "description": "Publish a Feature proposal only after a separate user approval moved it to Publishing.",
            "inputSchema": {
                "type": "object",
                "required": ["featureId"],
                "properties": { "featureId": { "type": "string", "format": "uuid" } },
                "additionalProperties": false
            }
        },
        {
            "name": "work_checkpoint",
            "description": "Record durable Work-item knowledge and its next action.",
            "inputSchema": {
                "type": "object",
                "required": ["workItemId", "nextAction", "summary", "idempotencyKey"],
                "properties": {
                    "workItemId": { "type": "string", "format": "uuid" },
                    "nextAction": {
                        "type": "string",
                        "enum": ["actionable", "blocked", "paused", "review", "delivery"]
                    },
                    "summary": { "type": "string", "minLength": 1 },
                    "idempotencyKey": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "session_request",
            "description": "Request and launch a fresh managed native session for an assigned Work item.",
            "inputSchema": {
                "type": "object",
                "required": ["workItemId", "tool", "idempotencyKey"],
                "properties": {
                    "workItemId": { "type": "string", "format": "uuid" },
                    "repositoryId": { "type": "string", "format": "uuid" },
                    "tool": { "type": "string", "enum": ["claude", "codex"] },
                    "idempotencyKey": { "type": "string", "minLength": 1 },
                    "terminal": { "type": "string" },
                    "native": { "type": "string" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "session_send_follow_up",
            "description": "Queue and deliver an ordered follow-up using only Workboard owner and session identity.",
            "inputSchema": {
                "type": "object",
                "required": ["owner", "expectedBindingGeneration", "text", "idempotencyKey"],
                "properties": {
                    "owner": {
                        "type": "object",
                        "required": ["kind", "id"],
                        "properties": {
                            "kind": { "type": "string", "enum": ["feature", "work_item"] },
                            "id": { "type": "string", "format": "uuid" }
                        },
                        "additionalProperties": false
                    },
                    "sessionId": { "type": "string", "format": "uuid" },
                    "expectedBindingGeneration": { "type": "integer", "minimum": 1 },
                    "text": { "type": "string", "minLength": 1, "maxLength": 65536 },
                    "idempotencyKey": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false
            }
        }
    ])
}

fn call_tool(application: &mut WorkboardApplication, request: &Value) -> Result<Value, AppError> {
    let name = request
        .pointer("/params/name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Domain("MCP tool name is missing".to_owned()))?;
    let arguments = request
        .pointer("/params/arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let token = workflow_token()?;
    let now = OffsetDateTime::now_utc();
    let result = match name {
        "hierarchy_read" => serde_json::to_value(application.assigned_hierarchy(&token, now)?)?,
        "epic_propose" => {
            let mut request: Value = arguments;
            request["proposedAt"] = json!(now.format(&Rfc3339).unwrap_or_default());
            serde_json::to_value(
                application
                    .workspace_planning()
                    .propose_epic(&token, serde_json::from_value(request)?)?,
            )?
        }
        "epic_propose_research" => {
            let mut request: Value = arguments;
            request["proposedAt"] = json!(now.format(&Rfc3339).unwrap_or_default());
            serde_json::to_value(
                application
                    .workspace_planning()
                    .propose_epic_research(&token, serde_json::from_value(request)?)?,
            )?
        }
        "feature_propose" => {
            let mut request: Value = arguments;
            request["proposedAt"] = json!(now.format(&Rfc3339).unwrap_or_default());
            serde_json::to_value(
                application
                    .workspace_planning()
                    .propose_feature(&token, serde_json::from_value(request)?)?,
            )?
        }
        "feature_submit_proposal" => {
            let request: FeatureProposalRequest = serde_json::from_value(arguments)?;
            serde_json::to_value(application.planning_workflows().submit_proposal(
                request.feature_id,
                &token,
                request.proposal,
                &request.idempotency_key,
                now,
            )?)?
        }
        "feature_publish" => {
            let request: FeaturePublicationRequest = serde_json::from_value(arguments)?;
            let principal = application
                .workflow_operations()
                .authenticate(&token, now)?;
            if principal.owner != HierarchyOwner::Feature(request.feature_id) {
                return Err(AppError::WorkflowOperationUnauthorized);
            }
            serde_json::to_value(
                application
                    .planning_workflows()
                    .publish_approved(request.feature_id, now)?,
            )?
        }
        "work_checkpoint" => {
            let request: WorkItemCheckpointRequest = serde_json::from_value(arguments)?;
            serde_json::to_value(application.workflow_operations().checkpoint(
                &token,
                CheckpointWorkItem {
                    work_item_id: request.work_item_id,
                    next_action: request.next_action,
                    summary: request.summary,
                    idempotency_key: request.idempotency_key,
                    recorded_at: now,
                },
            )?)?
        }
        "session_request" => {
            let request: ManagedSessionRequest = serde_json::from_value(arguments)?;
            execute_managed_session_request(application, &token, request, now)?
        }
        "session_send_follow_up" => {
            let request: SessionFollowUpRequest = serde_json::from_value(arguments)?;
            let queued = application.follow_ups().queue_authenticated(
                &token,
                SendSessionFollowUp {
                    owner: request.owner,
                    session_id: request.session_id,
                    expected_binding_generation: request.expected_binding_generation,
                    text: request.text,
                    idempotency_key: request.idempotency_key,
                    requested_at: now,
                },
            )?;
            let outcome = application
                .follow_ups()
                .deliver_next(Some(queued.session_id), now, &SystemFollowUpExecutor)?
                .unwrap_or(queued);
            serde_json::to_value(outcome)?
        }
        _ => {
            return Err(AppError::External {
                code: "mcp_tool_not_found".to_owned(),
                message: format!("unknown MCP tool {name}"),
            });
        }
    };
    let text = serde_json::to_string_pretty(&result)?;
    Ok(json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": result,
        "isError": false
    }))
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::{handle, tool_definitions};
    use workboard_application::workspace::WorkboardApplication;

    #[test]
    fn advertises_every_versioned_workflow_operation() {
        let tools = tool_definitions();
        let names = tools
            .as_array()
            .expect("tool array")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "hierarchy_read",
                "epic_propose",
                "epic_propose_research",
                "feature_propose",
                "feature_submit_proposal",
                "feature_publish",
                "work_checkpoint",
                "session_request",
                "session_send_follow_up"
            ]
        );
    }

    #[test]
    fn session_request_advertises_repository_selection() {
        let tools = tool_definitions();
        let session_request = tools
            .as_array()
            .expect("tool array")
            .iter()
            .find(|tool| tool["name"] == "session_request")
            .expect("session request tool");
        assert!(
            session_request["inputSchema"]["properties"]
                .get("repositoryId")
                .is_some()
        );
        let checkpoint = tools
            .as_array()
            .expect("tool array")
            .iter()
            .find(|tool| tool["name"] == "work_checkpoint")
            .expect("checkpoint tool");
        assert!(
            checkpoint["inputSchema"]["properties"]
                .get("repositoryId")
                .is_none()
        );
    }

    #[test]
    fn negotiates_initialize_and_lists_tools() {
        let directory = TempDir::new().expect("temporary directory");
        let mut application = WorkboardApplication::open(directory.path().join("workboard.sqlite"))
            .expect("open Workboard");
        let initialized = handle(
            &mut application,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "test-version" }
            }),
        )
        .expect("initialize response");
        assert_eq!(initialized["result"]["protocolVersion"], "test-version");
        let listed = handle(
            &mut application,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list"
            }),
        )
        .expect("tools response");
        assert_eq!(listed["result"]["tools"].as_array().map(Vec::len), Some(9));
    }
}
