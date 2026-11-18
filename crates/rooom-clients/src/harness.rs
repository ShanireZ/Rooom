use serde::Deserialize;

pub const DESKTOP_VERSION: &str = "0.2.2-rc.3";
pub const CHANNEL_VERSION: u32 = 1;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckIn {
    pub desktop_version: String,
    pub channel_version: u32,
    pub agent: Agent,
    pub at: u64,
    pub title: String,
    pub pid: Option<u32>,
    pub renderer: Option<Renderer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Renderer {
    pub pid: u32,
    pub at: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub id: String,
    pub parent: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "event", deny_unknown_fields)]
pub enum Event {
    #[serde(rename = "approval/request", rename_all = "camelCase")]
    Approval {
        agent: Agent,
        call_id: String,
        tool_name: String,
        reason: String,
        at: u64,
        #[serde(default)]
        cwd: serde_json::Value,
    },
    #[serde(rename = "assistant/message", rename_all = "camelCase")]
    Message {
        agent: Agent,
        message_id: String,
        text: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Delivery {
    pub request_id: String,
    pub session_id: String,
    pub stage: Stage,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Accepted,
    Started,
    Refused,
    Expired,
}

fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn agent_valid(agent: &Agent) -> bool {
    id(&agent.id) && agent.parent.is_none()
}

fn short(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096
}

pub fn parse_check_in(raw: &[u8]) -> Result<CheckIn, &'static str> {
    let item: CheckIn = serde_json::from_slice(raw).map_err(|_| "Malformed Harness check-in.")?;
    if item.desktop_version != DESKTOP_VERSION || item.channel_version != CHANNEL_VERSION {
        return Err("Unsupported Harness Desktop or plugin channel version.");
    }
    if !agent_valid(&item.agent) || !short(&item.title) || item.at == 0 {
        return Err("Invalid Harness session identity.");
    }
    Ok(item)
}

pub fn parse_event(raw: &[u8]) -> Result<Event, &'static str> {
    let event = serde_json::from_slice(raw).map_err(|_| "Malformed Harness event.")?;
    let valid = match &event {
        Event::Approval {
            agent,
            call_id,
            tool_name,
            reason,
            at,
            ..
        } => agent_valid(agent) && id(call_id) && short(tool_name) && short(reason) && *at != 0,
        Event::Message {
            agent,
            message_id,
            text,
        } => agent_valid(agent) && id(message_id) && short(text),
    };
    valid
        .then_some(event)
        .ok_or("Invalid Harness event identity.")
}

pub fn parse_delivery(raw: &[u8]) -> Result<Delivery, &'static str> {
    let item: Delivery = serde_json::from_slice(raw).map_err(|_| "Malformed Harness receipt.")?;
    (id(&item.request_id) && id(&item.session_id))
        .then_some(item)
        .ok_or("Invalid Harness receipt identity.")
}
