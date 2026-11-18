#![cfg(all(windows, feature = "adapter-fixtures"))]

use rooom_core::inbox::{Client, Decision, DeliveryState, SessionKey};
use rooom_platform::adapters::{
    Feed, MutationError,
    antigravity::{self, DeliveryTracker},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

struct Service {
    root: PathBuf,
    child: Option<Child>,
    tracker: RefCell<DeliveryTracker>,
}

impl Service {
    fn start(mode: &str) -> Self {
        static SEQUENCE: AtomicU32 = AtomicU32::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "rooom-m3-antigravity-{}-{}-{sequence}",
            now_ms(),
            std::process::id(),
        ));
        fs::create_dir(&root).expect("create isolated service fixture directory");
        let mut service = Self {
            root,
            child: None,
            tracker: RefCell::default(),
        };
        let helper = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/antigravity/service.ps1");
        let child = Command::new("pwsh")
            .creation_flags(CREATE_NO_WINDOW)
            .args(["-NoProfile", "-NonInteractive", "-File"])
            .arg(helper)
            .arg("-Root")
            .arg(&service.root)
            .args(["-Mode", mode])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start isolated service fixture");
        service.child = Some(child);
        let deadline = Instant::now() + Duration::from_secs(15);
        while !service.root.join("hub.json").is_file() {
            assert!(Instant::now() < deadline, "fixture service did not start");
            assert!(
                service
                    .child
                    .as_mut()
                    .unwrap()
                    .try_wait()
                    .unwrap()
                    .is_none(),
                "fixture service exited before publishing a hub"
            );
            thread::sleep(Duration::from_millis(25));
        }
        service
    }

    fn json(&self, filename: &str) -> Value {
        let data = fs::read(self.root.join(filename)).expect("read fixture record");
        serde_json::from_slice(&data).expect("parse fixture record")
    }

    fn send(&self) -> Result<rooom_core::inbox::Receipt, MutationError> {
        antigravity::send(
            &self.root,
            now_ms(),
            &target(),
            "delivery-1",
            "Fixture message.",
            &mut self.tracker.borrow_mut(),
        )
    }

    fn read(&self) -> Result<Feed, &'static str> {
        antigravity::read(&self.root, now_ms(), Some(&mut self.tracker.borrow_mut()))
    }

    fn publish_steps(&self, steps: &[Value]) {
        fs::write(
            self.root.join("trajectory.json"),
            serde_json::to_vec(steps).unwrap(),
        )
        .unwrap();
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        for name in [
            "hub.json",
            "hub.json.tmp",
            "writes.json",
            "writes.json.tmp",
            "change.request",
            "trajectory.json",
            "brain/session-1/.system_generated/logs/transcript.jsonl",
        ] {
            match fs::remove_file(self.root.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => panic!("could not remove fixture record {name}"),
            }
        }
        for name in [
            "brain/session-1/.system_generated/logs",
            "brain/session-1/.system_generated",
            "brain/session-1",
            "brain",
        ] {
            match fs::remove_dir(self.root.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => panic!("could not remove fixture directory {name}"),
            }
        }
        fs::remove_dir(&self.root).expect("remove empty service fixture directory");
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn target() -> SessionKey {
    SessionKey {
        client: Client::Antigravity,
        id: "session-1".into(),
    }
}

#[test]
fn native_history_follows_the_verified_conversation_without_writes() {
    let service = Service::start("native");
    let path = service
        .root
        .join("brain/session-1/.system_generated/logs/transcript.jsonl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let transcript = include_str!("../../../tests/fixtures/antigravity/transcript.jsonl");
    fs::write(&path, transcript).unwrap();

    let initial = service.read().unwrap();
    let timeline: Vec<_> = initial
        .timeline
        .iter()
        .map(|entry| {
            (
                entry.session.clone(),
                entry.id.as_str(),
                entry.text.as_str(),
            )
        })
        .collect();
    assert_eq!(
        timeline,
        [
            (target(), "step-1", "Review the synthetic results."),
            (target(), "step-3", "The synthetic results match."),
        ]
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), transcript);

    fs::write(&path, format!("{transcript}{{broken}}\n")).unwrap();
    let invalid = service.read().unwrap();
    assert!(invalid.warning.is_some());
    assert!(invalid.timeline.is_empty());
    assert_eq!(invalid.sessions[0].key, target());
    assert_eq!(invalid.requests.len(), 1);

    fs::write(&path, transcript).unwrap();
    let restored = service.read().unwrap();
    assert_eq!(restored.warning, None);
    assert_eq!(restored.timeline, initial.timeline);
    assert_eq!(
        service.json("writes.json"),
        json!({"decisions": [], "deliveries": []})
    );
}

#[test]
fn verified_service_answers_the_displayed_step_and_queues_the_selected_session() {
    let service = Service::start("normal");
    let initial = service.read().expect("verify running fixture service");
    assert_eq!(initial.sessions.len(), 1);
    assert_eq!(initial.sessions[0].key, target());
    assert_eq!(initial.requests.len(), 1);
    assert_eq!(initial.requests[0].key.id, "request-1");
    assert_eq!(
        initial.requests[0].workspace.as_deref(),
        Some("Antigravity Workspace 測試")
    );
    let mut mismatched_workspace = initial.requests[0].clone();
    mismatched_workspace.workspace = Some("Another workspace".into());
    assert_eq!(
        antigravity::answer_expected(
            &service.root,
            now_ms(),
            &mismatched_workspace,
            Decision::Allow
        ),
        Err(MutationError::Blocked)
    );
    assert_eq!(
        service.json("writes.json"),
        json!({"decisions": [], "deliveries": []})
    );

    fs::write(service.root.join("change.request"), []).unwrap();
    assert_eq!(
        antigravity::answer_expected(
            &service.root,
            now_ms(),
            &initial.requests[0],
            Decision::Allow
        ),
        Err(MutationError::Blocked)
    );
    assert_eq!(
        service.json("writes.json"),
        json!({"decisions": [], "deliveries": []})
    );

    let fresh = service.read().unwrap();
    assert_ne!(initial.requests[0].detail, fresh.requests[0].detail);
    antigravity::answer_expected(&service.root, now_ms(), &fresh.requests[0], Decision::Deny)
        .expect("answer freshly displayed waiting step");
    assert_eq!(
        antigravity::answer_expected(&service.root, now_ms(), &fresh.requests[0], Decision::Allow),
        Err(MutationError::Blocked)
    );
    assert!(service.read().unwrap().requests.is_empty());
    let receipt = service.send().expect("queue message on the same endpoint");
    assert_eq!(receipt.session, target());
    assert_eq!(receipt.state, DeliveryState::Queued);
    assert_eq!(
        receipt.external_id.as_deref(),
        Some("antigravity-message-1")
    );
    assert_eq!(
        service.json("writes.json"),
        json!({
            "decisions": [{
                "cascadeId": "session-1",
                "requestId": "request-1",
                "trajectoryId": "trajectory-fixture",
                "stepIndex": 3,
                "allow": false
            }],
            "deliveries": [{
                "cascadeId": "session-1",
                "deliveryId": "delivery-1",
                "messageId": "antigravity-message-1"
            }]
        })
    );
}

#[test]
fn blocks_a_stale_disabled_unknown_or_foreign_service_without_mutating_it() {
    let service = Service::start("normal");
    let initial = service.read().expect("baseline verified service");
    let hub = service.json("hub.json");
    for (key, value) in [
        ("issuedAtMs", json!(1)),
        ("enabled", json!(false)),
        ("desktopVersion", json!("unknown-desktop")),
        ("lsVersion", json!("unknown-service")),
        // The test process is same-user and same-session. It does not own the
        // server side of the connected socket.
        ("pid", json!(std::process::id())),
    ] {
        let mut rejected = hub.clone();
        rejected[key] = value;
        fs::write(service.root.join("hub.json"), rejected.to_string()).unwrap();
        assert!(service.read().is_err(), "{key}");
        assert_eq!(service.send(), Err(MutationError::Blocked), "{key}");
        assert_eq!(
            antigravity::answer_expected(
                &service.root,
                now_ms(),
                &initial.requests[0],
                Decision::Allow
            ),
            Err(MutationError::Blocked),
            "{key}"
        );
        assert_eq!(
            service.json("writes.json"),
            json!({"decisions": [], "deliveries": []})
        );
    }

    let foreign = Service::start("foreign");
    assert!(foreign.read().is_err());
    assert_eq!(foreign.send(), Err(MutationError::Blocked));
    assert_eq!(
        antigravity::answer_expected(
            &foreign.root,
            now_ms(),
            &initial.requests[0],
            Decision::Allow
        ),
        Err(MutationError::Blocked)
    );
    assert_eq!(
        foreign.json("writes.json"),
        json!({"decisions": [], "deliveries": []})
    );
}

#[test]
fn an_unconfirmed_write_is_uncertain() {
    let service = Service::start("lostSend");
    assert_eq!(service.send(), Err(MutationError::Uncertain));
    let writes = service.json("writes.json");
    assert_eq!(writes["deliveries"].as_array().unwrap().len(), 1);
    assert_eq!(writes["deliveries"][0]["deliveryId"], "delivery-1");
}

#[test]
fn native_ack_waits_for_a_new_matching_input_and_its_own_turn() {
    let service = Service::start("native");
    let planner = json!({"type": "CORTEX_STEP_TYPE_PLANNER_RESPONSE"});
    let mut steps = vec![
        input("Fixture message."),
        planner.clone(),
        json!({"type": "CORTEX_STEP_TYPE_GENERIC"}),
    ];
    service.publish_steps(&steps);
    let receipt = service
        .send()
        .expect("native HTTP acknowledgement queues the send");
    assert_eq!(receipt.session, target());
    assert_eq!(receipt.state, DeliveryState::Queued);
    assert_eq!(receipt.external_id, None);
    let observe = || service.read().unwrap().receipts;
    assert!(
        observe().is_empty(),
        "the pre-existing matching input is stale"
    );

    steps[2] = json!({"type": "CORTEX_STEP_TYPE_RUN_COMMAND"});
    steps.extend([input("Other text."), planner.clone()]);
    service.publish_steps(&steps);
    assert!(
        observe().is_empty(),
        "another turn is not delivery evidence"
    );

    steps.push(input("Fixture message."));
    steps[0] = input("Rewritten history.");
    service.publish_steps(&steps);
    assert!(
        observe().is_empty(),
        "rewritten user input is not the same history"
    );
    steps[0] = input("Fixture message.");
    service.publish_steps(&steps);
    let received = observe();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].id, receipt.id);
    assert_eq!(received[0].session, target());
    assert_eq!(received[0].state, DeliveryState::Received);

    steps.push(planner);
    service.publish_steps(&steps);
    let started = observe();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].state, DeliveryState::Started);
    assert_eq!(started[0].external_id, received[0].external_id);
    assert_eq!(
        service.json("writes.json")["deliveries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn native_lost_ack_observes_input_without_retrying_or_claiming_the_next_turn() {
    let service = Service::start("nativeLost");
    service.publish_steps(&[]);
    assert_eq!(service.send(), Err(MutationError::Uncertain));
    assert_eq!(service.send(), Err(MutationError::Blocked));
    service.publish_steps(&[
        input("Fixture message."),
        input("Unrelated following turn."),
        json!({"type": "CORTEX_STEP_TYPE_RUN_COMMAND"}),
    ]);
    let receipts = service.read().unwrap().receipts;
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].id, "delivery-1");
    assert_eq!(receipts[0].state, DeliveryState::Received);
    assert_eq!(
        service.json("writes.json")["deliveries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

fn input(text: &str) -> Value {
    json!({"type": "CORTEX_STEP_TYPE_USER_INPUT", "userInput": {"items": [{"text": text}]}})
}
