use rooom_clients::antigravity::{
    check_decision_ack, check_status, parse_history, parse_hub, parse_native_summaries,
    parse_send_ack, parse_summaries, send_body,
};
use serde_json::{Value, json};

const SUMMARIES: &[u8] = include_bytes!("../../../../tests/fixtures/antigravity/summaries.json");
const SEND_ACK: &[u8] = include_bytes!("../../../../tests/fixtures/antigravity/send-ack.json");
const NOW_MS: u64 = 1_000_000;

#[test]
fn native_summaries_can_be_untitled_and_project_only_the_selected_conversation() {
    let mut reply: Value = serde_json::from_slice(SUMMARIES).unwrap();
    let summaries = reply["trajectorySummaries"].as_object_mut().unwrap();
    let selected = summaries
        .get_mut("session-1")
        .unwrap()
        .as_object_mut()
        .unwrap();
    let title = selected.remove("title").unwrap();
    selected.insert("summary".into(), title.clone());
    summaries.insert("other-session".into(), json!({"waitingSteps": true}));
    let parsed = parse_native_summaries(reply.to_string().as_bytes(), Some("session-1")).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].title, title.as_str().unwrap());
    assert_eq!(parsed[0].permissions[0].id, "request-1");
    assert_eq!(
        parsed[0].workspace_uri.as_deref(),
        Some("file:///C:/synthetic/Antigravity%20Workspace%20%E6%B8%AC%E8%A9%A6")
    );
    reply["trajectorySummaries"]["session-1"]
        .as_object_mut()
        .unwrap()
        .remove("summary");
    let parsed = parse_native_summaries(reply.to_string().as_bytes(), Some("session-1")).unwrap();
    assert_eq!(parsed[0].title, "Antigravity");
    let selected = &mut reply["trajectorySummaries"]["session-1"];
    let workspace = selected["workspaces"][0].clone();
    selected["workspaces"] =
        json!([workspace, {"workspaceFolderAbsoluteUri":"file:///C:/elsewhere"}]);
    let parsed = parse_native_summaries(reply.to_string().as_bytes(), Some("session-1")).unwrap();
    assert!(parsed[0].workspace_uri.is_none());
}

#[test]
fn native_compatibility_requires_a_verified_exact_version_pair() {
    let mut record = fixture_hub();
    for (desktop, service, supported) in [
        ("2.32.1", "2.32.1", true),
        ("2.32.2", "2.32.2", true),
        ("2.32.2", "2.32.1", false),
        ("2.32.1", "2.32.2", false),
        ("2.32.3", "2.32.3", false),
    ] {
        record["desktopVersion"] = json!(desktop);
        record["lsVersion"] = json!(service);
        let hub = parse_hub(record.to_string().as_bytes(), NOW_MS);
        assert_eq!(
            hub.is_ok_and(|hub| hub.uses_trajectory_receipts()),
            supported
        );
    }
}

fn fixture_hub() -> Value {
    json!({
        "pid": 12,
        "httpPort": 40001,
        "desktopVersion": "2.29.0",
        "lsVersion": "2.31.0",
        "csrfToken": "synthetic-token",
        "instanceId": "fixture-instance",
        "issuedAtMs": NOW_MS - 1,
        "enabled": true
    })
}

#[test]
fn antigravity_correlates_session_waiting_step_and_queue_receipt() {
    let hub = parse_hub(&fixture_hub().to_string().into_bytes(), NOW_MS).unwrap();
    let status = json!({
        "pid": 12,
        "desktopVersion": "2.29.0",
        "lsVersion": "2.31.0",
        "instanceId": "fixture-instance",
        "userStatus": {"signedIn": true}
    });
    check_status(status.to_string().as_bytes(), &hub).unwrap();

    let conversations = parse_summaries(SUMMARIES).unwrap();
    let [conversation] = conversations.as_slice() else {
        panic!("expected one Antigravity session");
    };
    assert_eq!(conversation.id, "session-1");
    assert!(!conversation.running);
    let [permission] = conversation.permissions.as_slice() else {
        panic!("expected exactly one waiting step");
    };
    assert_eq!(permission.id, "request-1");
    let reply = permission.reply(false);
    assert_eq!(reply["cascadeId"], conversation.id);
    assert_eq!(reply["interaction"]["trajectoryId"], "trajectory-fixture");
    assert_eq!(reply["interaction"]["stepIndex"], 3);
    assert_eq!(reply["interaction"]["permission"]["allow"], false);
    assert_eq!(
        reply["interaction"]["permission"]["scope"],
        "PERMISSION_SCOPE_ONCE"
    );
    let decision_ack = json!({
        "cascadeId": "session-1",
        "requestId": "request-1",
        "trajectoryId": "trajectory-fixture",
        "stepIndex": 3,
        "allow": false,
    });
    check_decision_ack(decision_ack.to_string().as_bytes(), permission, false).unwrap();
    assert!(check_decision_ack(decision_ack.to_string().as_bytes(), permission, true).is_err());

    let trajectory = json!({
        "cascadeId": "session-1",
        "cascadeConfig": {"plannerConfig": {"model": "fixture-model"}}
    });
    let send = send_body(
        trajectory.to_string().as_bytes(),
        "session-1",
        "delivery-1",
        "Fixture words",
    )
    .unwrap();
    assert_eq!(
        send["deliveryStrategy"],
        "MESSAGE_DELIVERY_STRATEGY_WHEN_IDLE"
    );
    assert_eq!(send["cascadeConfig"], trajectory["cascadeConfig"]);
    assert_eq!(send["rooomDeliveryId"], "delivery-1");
    assert_eq!(
        parse_send_ack(SEND_ACK, "session-1", "delivery-1").unwrap(),
        "antigravity-message-1"
    );
}

#[test]
fn antigravity_rejects_unverified_service_and_mismatched_step_or_receipt() {
    let hub_record = fixture_hub();
    let hub = parse_hub(hub_record.to_string().as_bytes(), NOW_MS).unwrap();
    let status = json!({
        "pid": 12,
        "desktopVersion": "2.29.0",
        "lsVersion": "2.31.0",
        "instanceId": "foreign-instance",
        "userStatus": {"signedIn": true}
    });
    assert!(check_status(status.to_string().as_bytes(), &hub).is_err());
    for (key, value) in [
        ("enabled", json!(false)),
        ("issuedAtMs", json!(1)),
        ("lsVersion", json!("unverified-version")),
        ("desktopVersion", json!("unverified-version")),
        ("csrfToken", json!("header\r\ninjection")),
    ] {
        let mut bad_hub = hub_record.clone();
        bad_hub[key] = value;
        assert!(
            parse_hub(bad_hub.to_string().as_bytes(), NOW_MS).is_err(),
            "{key}"
        );
    }

    let source =
        "/trajectorySummaries/session-1/waitingSteps/0/step/metadata/sourceTrajectoryStepInfo";
    let mut invalid: Value = serde_json::from_slice(SUMMARIES).unwrap();
    invalid.pointer_mut(source).unwrap()["cascadeId"] = json!("elsewhere");
    assert!(parse_summaries(invalid.to_string().as_bytes()).is_err());
    invalid.pointer_mut(source).unwrap()["cascadeId"] = json!("session-1");
    invalid.pointer_mut(source).unwrap()["stepIndex"] = json!(4);
    assert!(parse_summaries(invalid.to_string().as_bytes()).is_err());
    assert!(parse_summaries(b"[]").is_err());
    assert!(parse_send_ack(SEND_ACK, "other-session", "delivery-1").is_err());
    assert!(parse_send_ack(SEND_ACK, "session-1", "other-delivery").is_err());
    assert!(parse_send_ack(b"{}", "session-1", "delivery-1").is_err());
}
#[test]
fn transcript_excludes_context_and_keeps_only_complete_distinct_text_steps() {
    let fixture = include_str!("../../../../tests/fixtures/antigravity/transcript.jsonl");
    let extra = r#"{"type":"USER_INPUT","source":"SYSTEM","status":"DONE","step_index":4,"content":"System input"}
{"type":"USER_INPUT","source":"USER_EXPLICIT","status":"DONE","step_index":5,"content":"<CONTEXT_SUMMARY>Internal notes</CONTEXT_SUMMARY>"}
{"type":"USER_INPUT","source":"USER_EXPLICIT","status":"DONE","step_index":6,"content":"<USER_REQUEST>Incomplete wrapper"}
{"type":"USER_INPUT","source":"USER_EXPLICIT","status":"DONE","step_index":7,"content":"Plain synthetic prompt"}
{"type":"PLANNER_RESPONSE","source":"MODEL","status":"RUNNING","step_index":8,"content":"Unfinished response"}"#;
    let bytes = format!("{fixture}{fixture}{extra}\n{{");
    let messages = parse_history(bytes.as_bytes()).unwrap();
    let projected: Vec<_> = messages
        .iter()
        .map(|message| (message.step, message.text.as_str()))
        .collect();
    assert_eq!(
        projected,
        [
            (1, "Review the synthetic results."),
            (3, "The synthetic results match."),
            (7, "Plain synthetic prompt"),
        ]
    );
    assert!(parse_history(format!("{bytes}\n").as_bytes()).is_err());
}

#[test]
fn transcript_bounds_the_number_of_recent_steps() {
    use std::fmt::Write as _;
    let mut records = String::new();
    for step in 0..300 {
        writeln!(
            records,
            r#"{{"type":"PLANNER_RESPONSE","source":"MODEL","status":"DONE","step_index":{step},"content":"Synthetic reply"}}"#
        )
        .unwrap();
    }
    let messages = parse_history(records.as_bytes()).unwrap();
    assert_eq!(messages.len(), 256);
    assert_eq!(messages.first().unwrap().step, 44);
    assert_eq!(messages.last().unwrap().step, 299);
}
