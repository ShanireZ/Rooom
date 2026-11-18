//! Antigravity fixture and verified Desktop delivery and transcript formats.

use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    hash::{DefaultHasher, Hash, Hasher},
};

pub const DESKTOP_VERSION: &str = "2.29.0";
pub const SERVICE_VERSION: &str = "2.31.0";
pub const WHEN_IDLE: &str = "MESSAGE_DELIVERY_STRATEGY_WHEN_IDLE";
pub const HISTORY_WINDOW: usize = 4 * 1024 * 1024;
const HUB_MAX_AGE_MS: u64 = 600_000;
const HISTORY_MESSAGES: usize = 256;

pub struct HistoryMessage {
    pub step: u64,
    pub text: String,
}

pub fn parse_history(bytes: &[u8]) -> Result<Vec<HistoryMessage>, &'static str> {
    if bytes.len() > HISTORY_WINDOW {
        return Err("Antigravity history exceeds its size limit.");
    }
    let mut entries = VecDeque::new();
    let mut seen = BTreeSet::new();
    let mut lines = bytes.split(|byte| *byte == b'\n');
    // The final segment has no terminating newline and may still be written.
    lines.next_back();
    for line in lines.filter(|line| !line.iter().all(u8::is_ascii_whitespace)) {
        let record: Value = decode(line)?;
        if !record.is_object() || !record["type"].is_string() {
            return Err("Antigravity history contains an invalid record.");
        }
        if record["status"] != "DONE" {
            continue;
        }
        let Some(raw) = record["content"].as_str() else {
            continue;
        };
        let text = match (record["type"].as_str(), record["source"].as_str()) {
            (Some("USER_INPUT"), Some("USER_EXPLICIT")) => user_text(raw),
            (Some("PLANNER_RESPONSE"), Some("MODEL")) => Some(raw),
            _ => None,
        };
        let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) else {
            continue;
        };
        let step = record["step_index"]
            .as_u64()
            .ok_or("Antigravity history contains an invalid step index.")?;
        if seen.insert(step) {
            if entries.len() == HISTORY_MESSAGES {
                entries.pop_front();
            }
            entries.push_back(HistoryMessage {
                step,
                text: text.to_owned(),
            });
        }
    }
    Ok(entries.into())
}

fn user_text(raw: &str) -> Option<&str> {
    let text = raw.trim_start();
    if let Some(request) = text.strip_prefix("<USER_REQUEST>") {
        return request.split_once("</USER_REQUEST>").map(|(body, _)| body);
    }
    let markers = [
        "USER_REQUEST",
        "ADDITIONAL_METADATA",
        "USER_SETTINGS_CHANGE",
        "CONTEXT_SUMMARY",
    ];
    (!text.split('<').skip(1).any(|fragment| {
        let tag = fragment.strip_prefix('/').unwrap_or(fragment);
        markers.iter().any(|marker| tag.starts_with(marker))
    }))
    .then_some(text)
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Hub {
    pub pid: u32,
    pub http_port: u16,
    pub desktop_version: String,
    pub ls_version: String,
    pub csrf_token: String,
    pub instance_id: String,
    pub issued_at_ms: u64,
    pub enabled: bool,
}

impl Hub {
    pub fn uses_trajectory_receipts(&self) -> bool {
        self.desktop_version == self.ls_version && supported_native_version(&self.desktop_version)
    }
}

pub fn supported_native_version(version: &str) -> bool {
    matches!(version, "2.32.1" | "2.32.2")
}

pub fn parse_hub(bytes: &[u8], now_ms: u64) -> Result<Hub, &'static str> {
    let hub: Hub = decode(bytes)?;
    if !hub.enabled {
        return Err("Antigravity is disabled.");
    }
    if !hub.uses_trajectory_receipts()
        && (hub.desktop_version != DESKTOP_VERSION || hub.ls_version != SERVICE_VERSION)
    {
        return Err("This Antigravity Desktop or language service version is unsupported.");
    }
    if !matches!(
        now_ms.checked_sub(hub.issued_at_ms),
        Some(0..=HUB_MAX_AGE_MS)
    ) {
        return Err("The Antigravity service announcement is stale.");
    }
    if hub.pid == 0
        || hub.http_port == 0
        || !valid_id(&hub.csrf_token)
        || !valid_id(&hub.instance_id)
    {
        return Err("The Antigravity service announcement is invalid.");
    }
    Ok(hub)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    user_status: BTreeMap<String, Value>,
    pid: u32,
    instance_id: String,
    desktop_version: String,
    ls_version: String,
}

pub fn check_status(bytes: &[u8], hub: &Hub) -> Result<(), &'static str> {
    if hub.uses_trajectory_receipts() {
        // The native reply has no synthetic instance/owner fields. The Windows
        // transport checks the PID that owns the exact socket before each RPC.
        let reply: Value = decode(bytes)?;
        return reply["userStatus"]
            .as_object()
            .is_some_and(|status| !status.is_empty())
            .then_some(())
            .ok_or("The Antigravity service did not return user status.");
    }
    let reply: Status = decode(bytes)?;
    if !reply.user_status.is_empty()
        && reply.pid == hub.pid
        && reply.instance_id == hub.instance_id
        && reply.desktop_version == hub.desktop_version
        && reply.ls_version == hub.ls_version
    {
        Ok(())
    } else {
        Err("The Antigravity service identity did not match its announcement.")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Summaries {
    trajectory_summaries: BTreeMap<String, Cascade>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cascade {
    #[serde(default, alias = "summary")]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    workspaces: Value,
    #[serde(default)]
    waiting_steps: Vec<Waiting>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Waiting {
    #[serde(default)]
    step_index: u32,
    step: Step,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Step {
    status: String,
    metadata: Metadata,
    requested_interaction: Interaction,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    #[serde(default)]
    execution_id: String,
    source_trajectory_step_info: Source,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Source {
    cascade_id: String,
    trajectory_id: String,
    #[serde(default)]
    step_index: u32,
}

#[derive(Deserialize)]
struct Interaction {
    permission: Permission,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Permission {
    action_description: String,
    resource: Resource,
}

#[derive(Deserialize)]
struct Resource {
    action: String,
    target: String,
}

pub struct Conversation {
    pub id: String,
    pub title: String,
    pub workspace_uri: Option<String>,
    pub running: bool,
    pub permissions: Vec<WaitingPermission>,
}

pub struct WaitingPermission {
    pub id: String,
    pub cascade_id: String,
    pub trajectory_id: String,
    pub step_index: u32,
    pub summary: String,
    pub detail: String,
}

impl WaitingPermission {
    pub fn reply(&self, allow: bool) -> Value {
        json!({
            "cascadeId": self.cascade_id,
            "interaction": {
                "trajectoryId": self.trajectory_id,
                "stepIndex": self.step_index,
                "permission": {"allow": allow, "scope": "PERMISSION_SCOPE_ONCE"}
            }
        })
    }
}

pub fn parse_summaries(bytes: &[u8]) -> Result<Vec<Conversation>, &'static str> {
    let Summaries {
        trajectory_summaries,
    } = decode(bytes)?;
    conversations(trajectory_summaries, false)
}

pub fn parse_native_summaries(
    bytes: &[u8],
    selected: Option<&str>,
) -> Result<Vec<Conversation>, &'static str> {
    let reply: Value = decode(bytes)?;
    let Some(summaries) = reply["trajectorySummaries"].as_object() else {
        return if reply.as_object().is_some_and(|map| map.is_empty()) {
            Ok(Vec::new())
        } else {
            Err("Antigravity returned an invalid trajectory list.")
        };
    };
    let mut decoded = BTreeMap::new();
    for (id, value) in summaries {
        if selected.is_some_and(|selected| selected != id) {
            continue;
        }
        let cascade = serde_json::from_value(value.clone())
            .map_err(|_| "Antigravity returned an invalid conversation.")?;
        decoded.insert(id.clone(), cascade);
    }
    conversations(decoded, true)
}

fn conversations(
    summaries: BTreeMap<String, Cascade>,
    native: bool,
) -> Result<Vec<Conversation>, &'static str> {
    summaries
        .into_iter()
        .map(|(id, cascade)| {
            if !valid_id(&id)
                || (!native
                    && (cascade.title.trim().is_empty()
                        || !matches!(
                            cascade.status.as_str(),
                            "CASCADE_RUN_STATUS_IDLE" | "CASCADE_RUN_STATUS_RUNNING"
                        )))
            {
                return Err("An Antigravity conversation has an invalid identity.");
            }
            let mut request_ids = BTreeSet::new();
            let mut waits = Vec::new();
            for waiting in cascade.waiting_steps {
                let metadata = waiting.step.metadata;
                let source = metadata.source_trajectory_step_info;
                let permission = waiting.step.requested_interaction.permission;
                let request_id = metadata.execution_id;
                if waiting.step.status != "CORTEX_STEP_STATUS_WAITING"
                    || source.cascade_id != id
                    || source.step_index != waiting.step_index
                    || !valid_id(&source.trajectory_id)
                    || !valid_id(&request_id)
                    || !request_ids.insert(request_id.clone())
                    || permission.resource.action.trim().is_empty()
                    || permission.resource.target.trim().is_empty()
                    || permission.action_description.trim().is_empty()
                {
                    return Err("An Antigravity waiting step has an invalid identity.");
                }
                waits.push(WaitingPermission {
                    id: request_id,
                    cascade_id: source.cascade_id,
                    trajectory_id: source.trajectory_id,
                    step_index: source.step_index,
                    summary: permission.action_description,
                    detail: permission.resource.target,
                });
            }
            Ok(Conversation {
                id,
                title: if cascade.title.trim().is_empty() {
                    "Antigravity".into()
                } else {
                    cascade.title
                },
                workspace_uri: single_workspace_uri(&cascade.workspaces),
                running: cascade.status == "CASCADE_RUN_STATUS_RUNNING",
                permissions: waits,
            })
        })
        .collect()
}

fn single_workspace_uri(value: &Value) -> Option<String> {
    let [workspace] = value.as_array()?.as_slice() else {
        return None;
    };
    workspace["workspaceFolderAbsoluteUri"]
        .as_str()
        .map(str::to_owned)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Trajectory {
    cascade_id: String,
    cascade_config: BTreeMap<String, Value>,
}

pub fn send_body(
    bytes: &[u8],
    cascade_id: &str,
    delivery_id: &str,
    text: &str,
) -> Result<Value, &'static str> {
    let trajectory: Trajectory = decode(bytes)?;
    if trajectory.cascade_id != cascade_id
        || trajectory.cascade_config.is_empty()
        || !valid_id(delivery_id)
        || text.trim().is_empty()
    {
        return Err("The Antigravity send target or input is invalid.");
    }
    Ok(json!({
        "metadata": metadata(),
        "cascadeId": cascade_id,
        "cascadeConfig": trajectory.cascade_config,
        "deliveryStrategy": WHEN_IDLE,
        "items": [{"text": text}],
        "rooomDeliveryId": delivery_id
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendAck {
    cascade_id: String,
    delivery_id: String,
    message_id: String,
    state: Queued,
}

#[derive(Deserialize)]
enum Queued {
    #[serde(rename = "queued")]
    Yes,
}

pub fn parse_send_ack(
    bytes: &[u8],
    cascade_id: &str,
    delivery_id: &str,
) -> Result<String, &'static str> {
    let ack: SendAck = decode(bytes)?;
    let Queued::Yes = ack.state;
    if ack.cascade_id == cascade_id && ack.delivery_id == delivery_id && valid_id(&ack.message_id) {
        Ok(ack.message_id)
    } else {
        Err("The Antigravity queue acknowledgement did not match the send.")
    }
}

pub fn check_native_send_ack(bytes: &[u8]) -> Result<(), &'static str> {
    if bytes.is_empty()
        || decode::<BTreeMap<String, Value>>(bytes).is_ok_and(|body| body.is_empty())
    {
        Ok(())
    } else {
        Err("Antigravity returned an unexpected native send acknowledgement.")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrajectoryReply {
    cascade_id: Option<String>,
    trajectory: NativeTrajectory,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTrajectory {
    cascade_id: Option<String>,
    trajectory_id: Option<String>,
    #[serde(default)]
    steps: Vec<NativeStep>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeStep {
    #[serde(rename = "type")]
    kind: String,
    user_input: Option<NativeInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeInput {
    #[serde(default)]
    items: Vec<InputItem>,
    user_config: Option<Value>,
    last_user_config: Option<Value>,
}

#[derive(Deserialize, Hash)]
struct InputItem {
    text: Option<String>,
}

pub struct TrajectoryCursor {
    trajectory_id: Option<String>,
    step_count: usize,
    prefix: u64,
}

pub fn parse_native_trajectory(
    bytes: &[u8],
    cascade_id: &str,
) -> Result<NativeTrajectory, &'static str> {
    let reply: TrajectoryReply = decode(bytes)?;
    if [
        reply.cascade_id.as_deref(),
        reply.trajectory.cascade_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|actual| actual != cascade_id)
        || reply
            .trajectory
            .trajectory_id
            .as_deref()
            .is_some_and(|id| !valid_id(id))
    {
        return Err("The Antigravity trajectory does not match its conversation.");
    }
    Ok(reply.trajectory)
}

impl NativeTrajectory {
    pub fn cursor(&self) -> TrajectoryCursor {
        TrajectoryCursor {
            trajectory_id: self.trajectory_id.clone(),
            step_count: self.steps.len(),
            prefix: fingerprint(&self.steps),
        }
    }

    pub fn delivery_evidence(
        &self,
        before: &TrajectoryCursor,
        text: &str,
    ) -> Option<(String, bool)> {
        if self.trajectory_id != before.trajectory_id
            || fingerprint(self.steps.get(..before.step_count)?) != before.prefix
        {
            return None;
        }
        let mut matches = self.steps[before.step_count..]
            .iter()
            .enumerate()
            .filter(|(_, step)| {
                step.kind == "CORTEX_STEP_TYPE_USER_INPUT"
                && step.user_input.as_ref().is_some_and(|input| {
                    matches!(&input.items[..], [item] if item.text.as_deref() == Some(text))
                })
            });
        let (offset, _) = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        let index = before.step_count + offset;
        let started = self.steps[index + 1..]
            .iter()
            .take_while(|step| step.kind != "CORTEX_STEP_TYPE_USER_INPUT")
            .any(|step| {
                matches!(
                    step.kind.as_str(),
                    "CORTEX_STEP_TYPE_PLANNER_RESPONSE" | "CORTEX_STEP_TYPE_RUN_COMMAND"
                )
            });
        let label = self.trajectory_id.as_deref().unwrap_or("step");
        Some((format!("{label}:{index}"), started))
    }

    pub fn send_body(
        &self,
        cascade_id: &str,
        text: &str,
        status: &[u8],
        version: &str,
    ) -> Result<Value, &'static str> {
        let config = self
            .steps
            .iter()
            .rev()
            .filter_map(|step| step.user_input.as_ref())
            .flat_map(|input| [input.user_config.as_ref(), input.last_user_config.as_ref()])
            .flatten()
            .find(|config| has_model(config))
            .cloned()
            .or_else(|| default_native_config(status))
            .ok_or("The Antigravity conversation has no usable model configuration.")?;
        Ok(json!({
            "metadata": metadata_for(version),
            "cascadeId": cascade_id,
            "cascadeConfig": config,
            "deliveryStrategy": WHEN_IDLE,
            "items": [{"text": text}]
        }))
    }
}

fn fingerprint(steps: &[NativeStep]) -> u64 {
    let mut hash = DefaultHasher::new();
    // A native GENERIC tool step can be replaced in place as it finishes.
    // The previously observed user inputs and their positions must stay put.
    for (index, step) in steps.iter().enumerate() {
        if step.kind == "CORTEX_STEP_TYPE_USER_INPUT" {
            index.hash(&mut hash);
            step.user_input
                .as_ref()
                .map(|input| &input.items)
                .hash(&mut hash);
        }
    }
    hash.finish()
}

fn has_model(config: &Value) -> bool {
    nonempty(&config["plannerConfig"]["planModel"])
        || has_model_or_alias(&config["plannerConfig"]["requestedModel"])
}

fn has_model_or_alias(value: &Value) -> bool {
    nonempty(&value["model"]) || nonempty(&value["alias"])
}

fn nonempty(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.trim().is_empty())
}

fn default_native_config(status: &[u8]) -> Option<Value> {
    let status: Value = decode(status).ok()?;
    let data = &status["userStatus"]["cascadeModelConfigData"];
    let offered = data["clientModelConfigs"].as_array();
    let default = &data["defaultOverrideModelConfig"]["modelOrAlias"];
    let allowed =
        |item: &Value| item["disabled"] != true && has_model_or_alias(&item["modelOrAlias"]);
    let model = if has_model_or_alias(default)
        && !offered.is_some_and(|list| {
            list.iter()
                .any(|item| item["disabled"] == true && item["modelOrAlias"] == *default)
        }) {
        default
    } else {
        let list = offered?;
        &list
            .iter()
            .find(|item| allowed(item) && item["isRecommended"] == true)
            .or_else(|| list.iter().find(|item| allowed(item)))?["modelOrAlias"]
    };
    Some(json!({"plannerConfig": {"requestedModel": model}}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DecisionAck {
    cascade_id: String,
    request_id: String,
    trajectory_id: String,
    step_index: u32,
    allow: bool,
}

pub fn check_decision_ack(
    bytes: &[u8],
    expected: &WaitingPermission,
    allow: bool,
) -> Result<(), &'static str> {
    let ack: DecisionAck = decode(bytes)?;
    if ack.cascade_id == expected.cascade_id
        && ack.request_id == expected.id
        && ack.trajectory_id == expected.trajectory_id
        && ack.step_index == expected.step_index
        && ack.allow == allow
    {
        Ok(())
    } else {
        Err("The Antigravity decision acknowledgement did not match the waiting step.")
    }
}

pub fn metadata() -> Value {
    metadata_for(DESKTOP_VERSION)
}

pub fn metadata_for(version: &str) -> Value {
    json!({
        "ideName": "antigravity",
        "extensionName": "antigravity",
        "locale": "en",
        "ideVersion": version
    })
}

fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, &'static str> {
    serde_json::from_slice(bytes).map_err(|_| "Antigravity returned malformed data.")
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
