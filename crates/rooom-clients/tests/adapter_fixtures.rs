#[path = "fixtures/antigravity.rs"]
mod antigravity;

use rooom_clients::{claude, codex, harness};

#[test]
fn claude_desktop_registry_hook_and_plugin_receipt_match() {
    let registry = claude::parse_registry(include_bytes!(
        "../../../tests/fixtures/claude/registry.json"
    ))
    .unwrap();
    let hook = claude::parse_hook(include_bytes!(
        "../../../tests/fixtures/claude/permission-request.json"
    ))
    .unwrap();
    let receipt = claude::parse_receipt(include_bytes!(
        "../../../tests/fixtures/claude/plugin-receipt.json"
    ))
    .unwrap();
    assert_eq!(registry.session_id, "session-1");
    assert_eq!(registry.version, "2.1.325");
    assert_eq!(hook.session_id, registry.session_id);
    assert_eq!(hook.prompt_id, "request-1");
    assert_eq!(receipt.session_id, registry.session_id);
    assert!(claude::parse_registry(b"[]").is_err());
    assert!(claude::parse_hook(br#"{"hook_event_name":"PreToolUse"}"#).is_err());
    assert!(claude::parse_receipt(b"{}").is_err());
}

#[test]
fn claude_transcript_keeps_complete_text_records_in_native_order() {
    let history = include_bytes!("../../../tests/fixtures/claude/history.jsonl");
    let mut log = history.to_vec();
    log.extend_from_slice(history);
    log.extend_from_slice(br#"{"type":"assistant","message":{"content": ["#);
    let messages = claude::parse_history(&log, "session-1").unwrap();
    let text: Vec<_> = messages
        .iter()
        .map(|m| (m.id.as_str(), m.text.as_str()))
        .collect();
    assert_eq!(
        text,
        [
            ("history-prompt-1", "Synthetic transcript prompt."),
            ("history-reply-1", "Synthetic transcript reply."),
        ]
    );
    assert!(claude::parse_history(&log, "other-session").is_err());
    log.push(b'\n');
    assert!(claude::parse_history(&log, "session-1").is_err());
}

#[test]
fn codex_rollout_command_request_and_stdout_receipt_match() {
    let rollout =
        codex::parse_rollout(include_str!("../../../tests/fixtures/codex/thread.jsonl")).unwrap();
    let owner =
        codex::parse_owner(include_str!("../../../tests/fixtures/codex/owner.json")).unwrap();
    let approval =
        codex::parse_approval(include_str!("../../../tests/fixtures/codex/approval.json")).unwrap();
    let request = approval.command().unwrap();
    let receipt =
        codex::QueueReceipt::parse_stdout("Queued message queue-1 for thread session-1.").unwrap();
    assert_eq!(owner.conversation_id, rollout.thread.id);
    assert_eq!(request.id, "request-1");
    assert_eq!(request.params.thread_id, rollout.thread.id);
    assert_eq!(receipt.thread_id, rollout.thread.id);
    assert!(codex::parse_rollout("{bad").is_err());
    assert!(codex::parse_approval("[]").is_err());
    assert!(codex::QueueReceipt::parse_stdout("Queued").is_err());
}

#[test]
fn codex_rollout_projects_the_durable_input_once_before_the_reply() {
    let rollout =
        codex::parse_rollout(include_str!("../../../tests/fixtures/codex/thread.jsonl")).unwrap();
    assert_eq!(
        rollout.thread.cwd.as_deref(),
        Some(r"C:\synthetic\Codex Workspace")
    );
    let texts: Vec<_> = rollout
        .messages
        .iter()
        .map(|message| (message.id.as_str(), message.text.as_str()))
        .collect();
    assert_eq!(
        texts,
        [
            ("input:item-prompt", "Review the synthetic Codex results."),
            ("item-intro", "Synthetic Codex timeline entry."),
        ]
    );
}

#[test]
fn harness_check_in_live_approval_and_controller_receipt_match() {
    let session = harness::parse_check_in(include_bytes!(
        "../../../tests/fixtures/harness/check-in.json"
    ))
    .unwrap();
    assert_eq!(session.agent.id, "session-1");
    assert!(session.renderer.is_none());
    let harness::Event::Approval {
        agent,
        call_id,
        cwd,
        ..
    } = harness::parse_event(include_bytes!(
        "../../../tests/fixtures/harness/approval.json"
    ))
    .unwrap()
    else {
        panic!("Expected an approval event");
    };
    assert_eq!(agent.id, session.agent.id);
    assert_eq!(call_id, "request-1");
    assert_eq!(cwd.as_str(), Some(r"C:\synthetic\Harness Workspace 測試"));
    let receipt = harness::parse_delivery(include_bytes!(
        "../../../tests/fixtures/harness/receipt.json"
    ))
    .unwrap();
    assert_eq!(receipt.session_id, session.agent.id);
    assert_eq!(receipt.request_id, "delivery-1");
    assert!(matches!(receipt.stage, harness::Stage::Accepted));
    assert!(harness::parse_check_in(br#"{"agent":{"id":"../other"}}"#).is_err());
    assert!(harness::parse_event(br#"{"event":"approval/request","callId":"request-1"}"#).is_err());
    assert!(harness::parse_delivery(b"{}").is_err());
}

#[test]
fn harness_check_in_can_report_the_desktop_renderer_separately_from_the_host() {
    let fixture = include_bytes!("../../../tests/fixtures/harness/check-in.json");
    let mut payload: serde_json::Value = serde_json::from_slice(fixture).unwrap();
    payload["pid"] = 202.into();
    payload["renderer"] = serde_json::json!({ "pid": 101, "at": 1 });
    let check = harness::parse_check_in(&serde_json::to_vec(&payload).unwrap()).unwrap();
    assert_eq!(check.pid, Some(202));
    let renderer = check.renderer.unwrap();
    assert_eq!(renderer.pid, 101);
    assert_eq!(renderer.at, 1);
}

#[test]
fn harness_approval_still_parses_without_a_usable_cwd() {
    let fixture = include_bytes!("../../../tests/fixtures/harness/approval.json");
    let mut payload: serde_json::Value = serde_json::from_slice(fixture).unwrap();
    payload.as_object_mut().unwrap().remove("cwd");
    for value in [None, Some(serde_json::json!({ "path": "invalid" }))] {
        if let Some(value) = value {
            payload["cwd"] = value;
        }
        let event = harness::parse_event(&serde_json::to_vec(&payload).unwrap()).unwrap();
        assert!(matches!(event, harness::Event::Approval { cwd, .. } if cwd.as_str().is_none()));
    }
}
