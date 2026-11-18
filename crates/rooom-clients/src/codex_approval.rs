//! The pinned Codex Desktop v13 snapshot subset used for user-reviewed approvals.

use crate::codex::identifier;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const COMMAND: &str = "item/commandExecution/requestApproval";
const FILE: &str = "item/fileChange/requestApproval";
pub const APPROVAL_WAIT_MS: u64 = 600_000;
pub const STREAM_VERSION: u32 = 13;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Command,
    File,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Choice {
    Allow,
    Deny,
    AllowForSession,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Approval {
    pub thread: String,
    pub turn: String,
    pub item: String,
    pub request: Value,
    pub kind: Kind,
    pub detail: String,
    pub session_grant: bool,
    wire: Value,
    reviewed_change: Value,
}

impl Approval {
    pub fn identity(&self) -> String {
        json!([self.thread, self.turn, self.item, self.request]).to_string()
    }

    pub fn reply(&self, choice: Choice) -> Option<(&'static str, Value)> {
        let decision = match choice {
            Choice::Allow => "accept",
            Choice::Deny => "decline",
            Choice::AllowForSession if self.kind == Kind::File && self.session_grant => {
                "acceptForSession"
            }
            Choice::AllowForSession => return None,
        };
        let method = match self.kind {
            Kind::Command => "thread-follower-command-approval-decision",
            Kind::File => "thread-follower-file-approval-decision",
        };
        Some((
            method,
            json!({
                "conversationId": self.thread, "requestId": self.request, "decision": decision
            }),
        ))
    }
}

pub fn acknowledged(response: &Value, owner: &str) -> bool {
    response["resultType"] == "success"
        && response["handledByClientId"] == owner
        && response["result"]["ok"] == true
}

pub fn requests(state: &Value, thread: &str) -> Option<Vec<Approval>> {
    let wires = state["requests"].as_array()?;
    if wires.is_empty() {
        return Some(Vec::new());
    }
    let turns = state["turnHistory"]["history"]["entitiesByKey"].as_object()?;
    let mut by_request = BTreeMap::new();
    for wire in wires {
        if let Some(card) = parse(wire, turns, thread) {
            let key = card.request.to_string();
            if by_request.insert(key, card).is_some() {
                return None;
            }
        }
    }
    Some(by_request.into_values().collect())
}

fn parse(wire: &Value, turns: &serde_json::Map<String, Value>, thread: &str) -> Option<Approval> {
    if wire["completed"] == true {
        return None;
    }
    let request = wire.get("id")?;
    if !request.is_i64() && !request.is_u64() && !request.as_str().is_some_and(identifier) {
        return None;
    }
    let params = &wire["params"];
    if params["threadId"] != thread {
        return None;
    }
    let turn = id(&params["turnId"])?;
    let item = id(&params["itemId"])?;
    let mut matches = turns.values().filter(|entry| entry["turnId"] == turn);
    let context = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    if context["params"]["approvalsReviewer"] != "user" || context["status"] != "inProgress" {
        return None;
    }
    let kind = match wire["method"].as_str()? {
        COMMAND => Kind::Command,
        FILE => Kind::File,
        _ => return None,
    };
    let (detail, reviewed_change, session_grant) = match kind {
        Kind::Command => {
            if !params["networkApprovalContext"].is_null()
                || !params["additionalPermissions"].is_null()
                || !allows(params, "accept")
            {
                return None;
            }
            let command = params["command"]
                .as_str()
                .filter(|s| !s.trim().is_empty())?;
            let detail = match params["cwd"].as_str() {
                Some(cwd) => format!("{cwd}\n\n{command}"),
                None => command.into(),
            };
            (detail, Value::Null, false)
        }
        Kind::File => {
            let tool = context["items"].as_array()?.iter().find(|entry| {
                entry["id"] == item
                    && entry["type"] == "fileChange"
                    && entry["status"] == "inProgress"
            })?;
            let changes = tool["changes"]
                .as_array()
                .filter(|changes| !changes.is_empty())?;
            if changes.iter().any(|change| {
                !change["path"]
                    .as_str()
                    .is_some_and(|path| !path.trim().is_empty())
                    || change["diff"].as_str().is_none()
            }) || !allows(params, "accept")
            {
                return None;
            }
            let detail = serde_json::to_string_pretty(&tool["changes"]).ok()?;
            let session_grant = params["grantRoot"].is_null() && allows(params, "acceptForSession");
            (detail, tool["changes"].clone(), session_grant)
        }
    };
    if detail.len() > 32 * 1024 {
        return None;
    }
    Some(Approval {
        thread: thread.into(),
        turn: turn.into(),
        item: item.into(),
        request: request.clone(),
        kind,
        detail,
        session_grant,
        wire: wire.clone(),
        reviewed_change,
    })
}

fn id(value: &Value) -> Option<&str> {
    value.as_str().filter(|id| identifier(id))
}

fn allows(params: &Value, choice: &str) -> bool {
    params["availableDecisions"].is_null()
        || params["availableDecisions"]
            .as_array()
            .is_some_and(|options| options.iter().any(|value| value == choice))
}

pub fn still_waiting(state: &Value, thread: &str, approval: &Approval) -> Option<bool> {
    Some(state["requests"].as_array()?.iter().any(|wire| {
        wire["id"] == approval.request
            && wire["params"]["threadId"] == thread
            && wire["completed"] != true
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(method: &str) -> Value {
        json!({
            "requests": [{
                "id": 42, "method": method,
                "params": {"threadId":"thread-1","turnId":"turn-1","itemId":"item-1","command":"Write-Output test"}
            }],
            "turnHistory": {"history":{"entitiesByKey":{"synthetic-turn":{
                "turnId":"turn-1", "status":"inProgress", "params":{"approvalsReviewer":"user"},
                "items":[{"id":"item-1","type":"fileChange","status":"inProgress","changes":[{"path":"test.txt","diff":"+test"}]}]
            }}}}
        })
    }

    #[test]
    fn exposes_only_the_approved_scopes_and_requires_complete_file_details() {
        let command = requests(&state(COMMAND), "thread-1").unwrap().remove(0);
        assert!(command.reply(Choice::AllowForSession).is_none());
        assert_eq!(command.reply(Choice::Allow).unwrap().1["requestId"], 42);
        let mut patch = state(FILE);
        let card = requests(&patch, "thread-1").unwrap().remove(0);
        assert_eq!(
            card.reply(Choice::AllowForSession).unwrap().1["decision"],
            "acceptForSession"
        );
        patch["requests"][0]["params"]["grantRoot"] = json!("C:/broader-root");
        assert!(
            requests(&patch, "thread-1")
                .unwrap()
                .remove(0)
                .reply(Choice::AllowForSession)
                .is_none()
        );
        patch["turnHistory"]["history"]["entitiesByKey"]["synthetic-turn"]["items"][0]["changes"] =
            json!([]);
        assert!(requests(&patch, "thread-1").unwrap().is_empty());
    }

    #[test]
    fn owner_receipt_must_match_and_foreign_review_is_not_projected() {
        let mut sample = state(COMMAND);
        let card = requests(&sample, "thread-1").unwrap().remove(0);
        let receipt = json!({"handledByClientId":"owner-1","resultType":"success",
            "result":{"ok":true}});
        assert!(acknowledged(&receipt, "owner-1"));
        assert!(!acknowledged(&receipt, "owner-2"));
        let mut failed = receipt;
        failed["result"]["ok"] = json!(false);
        assert!(!acknowledged(&failed, "owner-1"));
        sample["turnHistory"]["history"]["entitiesByKey"]["synthetic-turn"]["params"]["approvalsReviewer"] =
            json!("guardian");
        assert!(requests(&sample, "thread-1").unwrap().is_empty());
        assert_eq!(still_waiting(&sample, "thread-1", &card), Some(true));
        sample["requests"][0]["completed"] = json!(true);
        assert_eq!(still_waiting(&sample, "thread-1", &card), Some(false));
    }
}
