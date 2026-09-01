#![no_main]

use libfuzzer_sys::fuzz_target;
use workboard_application::hooks::{HookIngestionMutation, parse_hook};
use workboard_core::Tool;

fuzz_target!(|data: &[u8]| {
    let Ok(payload_json) = std::str::from_utf8(data) else {
        return;
    };
    let tool = if data.first().is_some_and(|byte| byte & 1 == 0) {
        Tool::Claude
    } else {
        Tool::Codex
    };
    let _ = parse_hook(&HookIngestionMutation {
        tool,
        payload_json: payload_json.to_owned(),
        observed_at: "1970-01-01T00:00:00Z".to_owned(),
        launch_token: None,
        process: None,
    });
});
