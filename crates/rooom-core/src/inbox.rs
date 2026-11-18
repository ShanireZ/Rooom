use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Client {
    Claude,
    Codex,
    Antigravity,
    Harness,
}

impl Client {
    pub const ALL: [Self; 4] = [Self::Claude, Self::Codex, Self::Antigravity, Self::Harness];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Antigravity => "antigravity",
            Self::Harness => "harness",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SessionKey {
    pub client: Client,
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RequestKey {
    pub session: SessionKey,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub key: SessionKey,
    pub title: String,
    pub send_blocked: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub key: RequestKey,
    pub summary: String,
    #[serde(default)]
    pub workspace: Option<String>,
    pub detail: String,
    pub decision_blocked: Option<String>,
    #[serde(default)]
    pub allow_for_session: bool,
    #[serde(default)]
    pub can_open_session: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TimelineEntry {
    pub session: SessionKey,
    pub id: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Allow,
    Deny,
    AllowForSession,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeliveryState {
    Queued,
    Received,
    Started,
    Uncertain,
    Expired,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub id: String,
    pub session: SessionKey,
    pub state: DeliveryState,
    pub external_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdapterStatus {
    pub client: Client,
    pub reason: Option<String>,
    pub setup: &'static str,
}

impl AdapterStatus {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            reason: Some("Awaiting verification with the installed client.".into()),
            setup: match client {
                Client::Claude => {
                    "Add the Rooom PermissionRequest command hook and Code plugin in Claude."
                }
                Client::Codex => {
                    "Use a thread owned by Codex Desktop with user-reviewed approvals."
                }
                Client::Antigravity => {
                    "Open a conversation in Antigravity Desktop so its local service is available."
                }
                Client::Harness => "Add the Rooom plugin entry to the Harness patch file yourself.",
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InboxSnapshot {
    pub adapters: Vec<AdapterStatus>,
    pub sessions: Vec<Session>,
    pub requests: Vec<Request>,
    pub timeline: Vec<TimelineEntry>,
    pub deliveries: Vec<Receipt>,
}

impl Default for InboxSnapshot {
    fn default() -> Self {
        Self {
            adapters: Client::ALL.into_iter().map(AdapterStatus::new).collect(),
            sessions: Vec::new(),
            requests: Vec::new(),
            timeline: Vec::new(),
            deliveries: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct Inbox {
    view: InboxSnapshot,
    closed: BTreeSet<RequestKey>,
    unanswered: BTreeSet<RequestKey>,
    // The in-flight marker is persisted as Uncertain before any adapter can write.
    in_flight: BTreeSet<String>,
}

impl Inbox {
    pub fn snapshot(&self) -> &InboxSnapshot {
        &self.view
    }

    pub fn replace_source(
        &mut self,
        client: Client,
        reason: Option<String>,
        sessions: Vec<Session>,
        requests: Vec<Request>,
        timeline: Vec<TimelineEntry>,
    ) -> bool {
        self.view
            .adapters
            .iter_mut()
            .find(|s| s.client == client)
            .unwrap()
            .reason = reason;
        let sessions: BTreeMap<_, _> = sessions
            .into_iter()
            .filter(|s| s.key.client == client)
            .map(|s| (s.key.clone(), s))
            .collect();
        let requests: BTreeMap<_, _> = requests
            .into_iter()
            .filter(|r| sessions.contains_key(&r.key.session) && !self.closed.contains(&r.key))
            .map(|mut r| {
                if self.unanswered.contains(&r.key) {
                    r.decision_blocked = Some("The client did not confirm the decision.".into());
                }
                r
            })
            .map(|r| (r.key.clone(), r))
            .collect();
        let mut timeline: BTreeMap<_, _> = timeline
            .into_iter()
            .filter(|entry| sessions.contains_key(&entry.session))
            .enumerate()
            .map(|(index, e)| ((e.session.clone(), e.id.clone()), (index, e)))
            .collect();
        let arrival = requests
            .keys()
            .any(|key| !self.view.requests.iter().any(|r| &r.key == key));
        self.view.sessions.retain(|s| s.key.client != client);
        self.view.sessions.extend(sessions.into_values());
        self.view.sessions.sort_by(|a, b| a.key.cmp(&b.key));
        self.view
            .requests
            .retain(|r| r.key.session.client != client);
        self.view.requests.extend(requests.into_values());
        self.view.requests.sort_by(|a, b| a.key.cmp(&b.key));
        if client == Client::Harness {
            // Insert recovered history before its next surviving same-session
            // message. Existing messages keep their inter-session positions.
            let surviving: BTreeSet<_> = self
                .view
                .timeline
                .iter()
                .map(|entry| (entry.session.clone(), entry.id.clone()))
                .filter(|key| timeline.contains_key(key))
                .collect();
            let mut ordered: Vec<_> = timeline.into_values().collect();
            ordered.sort_by_key(|(index, _)| *index);
            let mut anchors = BTreeMap::new();
            let mut updates = BTreeMap::<_, VecDeque<_>>::new();
            let mut before = BTreeMap::<_, Vec<_>>::new();
            let mut remaining = Vec::new();
            for (index, entry) in ordered.into_iter().rev() {
                let key = (entry.session.clone(), entry.id.clone());
                if surviving.contains(&key) {
                    anchors.insert(entry.session.clone(), key.clone());
                    updates
                        .entry(entry.session.clone())
                        .or_default()
                        .push_front(entry);
                } else if let Some(anchor) = anchors.get(&entry.session) {
                    before
                        .entry(anchor.clone())
                        .or_default()
                        .push((index, entry));
                } else {
                    remaining.push((index, entry));
                }
            }
            let previous = std::mem::take(&mut self.view.timeline);
            for entry in previous {
                if entry.session.client != client {
                    self.view.timeline.push(entry);
                } else if surviving.contains(&(entry.session.clone(), entry.id.clone())) {
                    let updated = updates
                        .get_mut(&entry.session)
                        .unwrap()
                        .pop_front()
                        .unwrap();
                    if let Some(mut recovered) =
                        before.remove(&(updated.session.clone(), updated.id.clone()))
                    {
                        recovered.sort_by_key(|(index, _)| *index);
                        self.view
                            .timeline
                            .extend(recovered.into_iter().map(|(_, entry)| entry));
                    }
                    self.view.timeline.push(updated);
                }
            }
            remaining.sort_by_key(|(index, _)| *index);
            self.view
                .timeline
                .extend(remaining.into_iter().map(|(_, entry)| entry));
            return arrival;
        }
        // Keep the order in which resident sources first observed entries.
        self.view.timeline.retain_mut(|entry| {
            if entry.session.client != client {
                return true;
            }
            if let Some((_, updated)) = timeline.remove(&(entry.session.clone(), entry.id.clone()))
            {
                *entry = updated;
                return true;
            }
            false
        });
        let mut fresh: Vec<_> = timeline.into_values().collect();
        fresh.sort_by_key(|(index, _)| *index);
        self.view
            .timeline
            .extend(fresh.into_iter().map(|(_, entry)| entry));
        arrival
    }

    pub fn can_open_request_session(&self, key: &RequestKey) -> bool {
        self.view
            .requests
            .iter()
            .any(|request| request.key == *key && request.can_open_session)
    }

    pub fn confirmed_answer(&mut self, key: &RequestKey) -> bool {
        if !self.view.requests.iter().any(|r| &r.key == key) {
            return false;
        }
        self.closed.insert(key.clone());
        self.view.requests.retain(|r| &r.key != key);
        true
    }

    pub fn restore(receipts: Vec<Receipt>) -> Option<Self> {
        let mut inbox = Self::default();
        let mut ids = BTreeSet::new();
        if receipts.iter().any(|r| {
            !identifier(&r.id)
                || !identifier(&r.session.id)
                || !ids.insert(&r.id)
                || (matches!(
                    r.state,
                    DeliveryState::Queued | DeliveryState::Received | DeliveryState::Started
                ) && !receipt_identity(r))
                || r.external_id.as_deref().is_some_and(|id| !identifier(id))
        }) {
            return None;
        }
        inbox.view.deliveries = receipts;
        Some(inbox)
    }

    pub fn mark_answer_uncertain(&mut self, key: &RequestKey) {
        self.unanswered.insert(key.clone());
        if let Some(request) = self.view.requests.iter_mut().find(|r| &r.key == key) {
            request.decision_blocked = Some("The client did not confirm the decision.".into());
        }
    }

    pub fn reserve_send(&mut self, id: &str, target: &SessionKey) -> bool {
        if !identifier(id)
            || !identifier(&target.id)
            || self.view.deliveries.iter().any(|r| r.id == id)
            || !self
                .view
                .sessions
                .iter()
                .any(|s| &s.key == target && s.send_blocked.is_none())
        {
            return false;
        }
        self.in_flight.insert(id.into());
        self.view.deliveries.push(Receipt {
            id: id.into(),
            session: target.clone(),
            state: DeliveryState::Uncertain,
            external_id: None,
        });
        true
    }

    pub fn finish_send(&mut self, id: &str, receipt: Option<Receipt>, blocked: bool) {
        if !self.in_flight.remove(id) {
            return;
        }
        if blocked {
            if let Some(entry) = self.view.deliveries.iter_mut().find(|r| r.id == id) {
                entry.state = DeliveryState::Rejected;
            }
        } else if let Some(receipt) = receipt.filter(|r| r.id == id) {
            self.record_evidence(receipt);
        }
    }

    pub fn record_evidence(&mut self, proof: Receipt) -> bool {
        use DeliveryState::*;
        if !receipt_identity(&proof) {
            return false;
        }
        let Some(existing) = self
            .view
            .deliveries
            .iter_mut()
            .find(|r| r.id == proof.id && r.session == proof.session)
        else {
            return false;
        };
        if existing
            .external_id
            .as_deref()
            .is_some_and(|previous| Some(previous) != proof.external_id.as_deref())
        {
            return false;
        }
        let valid = matches!(
            (existing.state, proof.state),
            (Uncertain, Queued | Received | Started | Expired | Rejected)
                | (Queued, Received | Started | Expired | Rejected)
                | (Received, Started | Rejected)
        );
        if valid {
            *existing = proof;
        }
        valid
    }
}

fn receipt_identity(receipt: &Receipt) -> bool {
    match receipt.external_id.as_deref() {
        Some(id) => identifier(id),
        None => {
            receipt.session.client == Client::Antigravity && receipt.state == DeliveryState::Queued
        }
    }
}

pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.'))
}
