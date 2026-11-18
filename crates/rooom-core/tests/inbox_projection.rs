use rooom_core::inbox::{
    Client, DeliveryState, Inbox, Receipt, Request, RequestKey, Session, SessionKey, TimelineEntry,
};

fn fixture(client: Client) -> (Session, Request, TimelineEntry) {
    let key = SessionKey {
        client,
        id: "session-1".into(),
    };
    (
        Session {
            key: key.clone(),
            title: "Fixture session".into(),
            send_blocked: None,
        },
        Request {
            key: RequestKey {
                session: key.clone(),
                id: "request-1".into(),
            },
            summary: "Synthetic request".into(),
            workspace: None,
            detail: "fixture command".into(),
            decision_blocked: None,
            allow_for_session: false,
            can_open_session: false,
        },
        TimelineEntry {
            session: key,
            id: "message-1".into(),
            text: "synthetic text".into(),
        },
    )
}

#[test]
fn repeated_ids_are_scoped_and_the_first_confirmed_answer_closes_only_its_card() {
    let mut inbox = Inbox::default();
    for client in Client::ALL {
        let (s, r, t) = fixture(client);
        assert!(inbox.replace_source(
            client,
            None,
            vec![s.clone(), s.clone()],
            vec![r.clone(), r.clone()],
            vec![t.clone(), t.clone()]
        ));
        assert!(!inbox.replace_source(client, None, vec![s], vec![r], vec![t]));
    }
    assert_eq!(inbox.snapshot().sessions.len(), 4);
    assert_eq!(inbox.snapshot().requests.len(), 4);
    assert_eq!(inbox.snapshot().timeline.len(), 4);
    let (s, r, t) = fixture(Client::Harness);
    assert!(inbox.confirmed_answer(&r.key));
    assert!(!inbox.confirmed_answer(&r.key));
    assert!(!inbox.replace_source(Client::Harness, None, vec![s], vec![r], vec![t]));
    assert_eq!(inbox.snapshot().requests.len(), 3);
}

#[test]
fn opening_a_session_requires_the_matching_pending_card_and_capability() {
    let mut inbox = Inbox::default();
    let (session, mut request, _) = fixture(Client::Codex);
    let (_, unrelated, _) = fixture(Client::Claude);
    inbox.replace_source(
        Client::Codex,
        None,
        vec![session.clone()],
        vec![request.clone()],
        vec![],
    );
    assert!(!inbox.can_open_request_session(&request.key));
    request.can_open_session = true;
    inbox.replace_source(
        Client::Codex,
        None,
        vec![session],
        vec![request.clone()],
        vec![],
    );
    assert!(inbox.can_open_request_session(&request.key));
    assert!(!inbox.can_open_request_session(&unrelated.key));
    assert!(inbox.confirmed_answer(&request.key));
    assert!(!inbox.can_open_request_session(&request.key));
}

#[test]
fn recovered_harness_history_orders_messages_even_after_a_plugin_only_cold_start() {
    let mut inbox = Inbox::default();
    let (session, _, plugin) = fixture(Client::Harness);
    let (_, _, earlier_other_client) = fixture(Client::Codex);
    let (other_session, _, _) = fixture(Client::Codex);
    inbox.replace_source(
        Client::Harness,
        Some("history unavailable".into()),
        vec![session.clone()],
        vec![],
        vec![plugin.clone()],
    );
    inbox.replace_source(
        Client::Codex,
        None,
        vec![other_session],
        vec![],
        vec![earlier_other_client.clone()],
    );
    let user = TimelineEntry {
        id: "native-user".into(),
        text: "synthetic prompt".into(),
        ..plugin.clone()
    };
    let answer = TimelineEntry {
        id: "native-answer".into(),
        text: "synthetic answer".into(),
        ..plugin.clone()
    };
    inbox.replace_source(
        Client::Harness,
        None,
        vec![session],
        vec![],
        vec![user.clone(), answer.clone(), plugin.clone()],
    );
    let harness: Vec<_> = inbox
        .snapshot()
        .timeline
        .iter()
        .filter(|entry| entry.session.client == Client::Harness)
        .cloned()
        .collect();
    assert_eq!(harness, vec![user, answer, plugin]);
    assert!(inbox.snapshot().timeline.contains(&earlier_other_client));
    let (session, _, second_plugin) = fixture(Client::Harness);
    let first_plugin = TimelineEntry {
        id: "earlier-plugin".into(),
        ..second_plugin.clone()
    };
    let mut cold = Inbox::default();
    cold.replace_source(
        Client::Harness,
        Some("history unavailable".into()),
        vec![session.clone()],
        vec![],
        vec![second_plugin.clone(), first_plugin.clone()],
    );
    cold.replace_source(
        Client::Harness,
        None,
        vec![session],
        vec![],
        vec![first_plugin.clone(), second_plugin.clone()],
    );
    assert_eq!(cold.snapshot().timeline, vec![first_plugin, second_plugin]);
}

#[test]
fn harness_history_recovery_preserves_order_between_live_sessions() {
    let mut inbox = Inbox::default();
    let (first, _, a1) = fixture(Client::Harness);
    let second = Session {
        key: SessionKey {
            client: Client::Harness,
            id: "session-2".into(),
        },
        ..first.clone()
    };
    let b1 = TimelineEntry {
        session: second.key.clone(),
        id: "b-1".into(),
        text: "B1".into(),
    };
    let a2 = TimelineEntry {
        id: "a-2".into(),
        text: "A2".into(),
        ..a1.clone()
    };
    let sessions = vec![first, second];
    inbox.replace_source(
        Client::Harness,
        None,
        sessions.clone(),
        vec![],
        vec![a1.clone(), b1.clone()],
    );
    inbox.replace_source(
        Client::Harness,
        None,
        sessions.clone(),
        vec![],
        vec![a1.clone(), a2.clone(), b1.clone()],
    );
    assert_eq!(inbox.snapshot().timeline, vec![a1, b1.clone(), a2.clone()]);
    let a3 = TimelineEntry {
        id: "a-3".into(),
        text: "A3".into(),
        ..a2.clone()
    };
    inbox.replace_source(
        Client::Harness,
        None,
        sessions,
        vec![],
        vec![a2.clone(), a3.clone(), b1.clone()],
    );
    assert_eq!(inbox.snapshot().timeline, vec![b1, a2, a3]);
}

#[test]
fn only_correlated_client_evidence_moves_a_delivery_forward() {
    let mut inbox = Inbox::default();
    let (session, _, _) = fixture(Client::Codex);
    let target = session.key.clone();
    inbox.replace_source(Client::Codex, None, vec![session], vec![], vec![]);
    assert!(inbox.reserve_send("delivery-1", &target));
    let proof = |state| Receipt {
        id: "delivery-1".into(),
        session: target.clone(),
        state,
        external_id: Some("native-1".into()),
    };
    assert_eq!(
        inbox.snapshot().deliveries[0].state,
        DeliveryState::Uncertain
    );
    inbox.finish_send("delivery-1", Some(proof(DeliveryState::Queued)), false);
    assert_eq!(inbox.snapshot().deliveries[0].state, DeliveryState::Queued);
    assert!(!inbox.record_evidence(Receipt {
        session: fixture(Client::Claude).0.key,
        ..proof(DeliveryState::Started)
    }));
    assert!(!inbox.record_evidence(Receipt {
        external_id: Some("native-2".into()),
        ..proof(DeliveryState::Started)
    }));
    assert!(inbox.record_evidence(proof(DeliveryState::Received)));
    assert_eq!(
        inbox.snapshot().deliveries[0].state,
        DeliveryState::Received
    );
    assert!(inbox.record_evidence(proof(DeliveryState::Started)));
    assert!(!inbox.record_evidence(proof(DeliveryState::Queued)));
    assert_eq!(inbox.snapshot().deliveries[0].state, DeliveryState::Started);
}

#[test]
fn an_uncertain_write_is_not_reissued_even_after_receipts_reload() {
    let mut inbox = Inbox::default();
    let (session, _, _) = fixture(Client::Antigravity);
    let target = session.key.clone();
    inbox.replace_source(
        Client::Antigravity,
        None,
        vec![session.clone()],
        vec![],
        vec![],
    );
    let mut writes = 0;
    for index in 0..2 {
        if inbox.reserve_send("delivery-1", &target) {
            writes += 1;
            inbox.finish_send("delivery-1", None, false);
        } else {
            assert_eq!(index, 1);
        }
    }
    let bytes = serde_json::to_vec(&inbox.snapshot().deliveries).unwrap();
    let mut reloaded = Inbox::restore(serde_json::from_slice(&bytes).unwrap()).unwrap();
    reloaded.replace_source(Client::Antigravity, None, vec![session], vec![], vec![]);
    assert!(!reloaded.reserve_send("delivery-1", &target));
    assert_eq!(writes, 1);
    assert_eq!(
        reloaded.snapshot().deliveries[0].state,
        DeliveryState::Uncertain
    );
}

#[test]
fn native_antigravity_queue_ack_can_reload_without_inventing_a_message_id() {
    let mut inbox = Inbox::default();
    let (session, _, _) = fixture(Client::Antigravity);
    let target = session.key.clone();
    inbox.replace_source(Client::Antigravity, None, vec![session], vec![], vec![]);
    assert!(inbox.reserve_send("delivery-1", &target));
    let queued = Receipt {
        id: "delivery-1".into(),
        session: target,
        state: DeliveryState::Queued,
        external_id: None,
    };
    inbox.finish_send("delivery-1", Some(queued.clone()), false);
    assert_eq!(inbox.snapshot().deliveries, [queued.clone()]);
    let mut restored = Inbox::restore(vec![queued.clone()]).unwrap();
    assert!(!restored.record_evidence(Receipt {
        state: DeliveryState::Received,
        ..queued.clone()
    }));
    assert!(restored.record_evidence(Receipt {
        state: DeliveryState::Received,
        external_id: Some("trajectory-fixture:2".into()),
        ..queued
    }));
}
