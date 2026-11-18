//! Claude Desktop Code's registry and PermissionRequest, plus Rooom's plugin channel.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::VecDeque;

pub const DESKTOP_VERSION: &str = "5.11492.5";
pub const ENGINE_VERSION: &str = "2.1.325";
pub const PLUGIN_VERSION: u32 = 1;
pub const PLUGIN_FRESH_MS: u64 = 15_000;
pub const DELIVERY_LIFETIME_MS: u64 = 3_600_000;
pub const RECORD_LIMIT: usize = 256 * 1024;
pub const HISTORY_WINDOW: usize = 4 * 1024 * 1024;
const HISTORY_MESSAGES: usize = 256;

#[derive(Debug, PartialEq, Eq)]
pub struct HistoryMessage {
    pub id: String,
    pub text: String,
}

/// Rebuild a bounded suffix of the native transcript from committed lines.
pub fn parse_history(bytes: &[u8], session: &str) -> Result<Vec<HistoryMessage>, &'static str> {
    if bytes.len() > HISTORY_WINDOW || !valid_id(session) {
        return Err("The Claude transcript window is invalid.");
    }
    let mut messages: VecDeque<HistoryMessage> = VecDeque::new();
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if !line.ends_with(b"\n")
            || line.len() > RECORD_LIMIT
            || line.iter().all(u8::is_ascii_whitespace)
        {
            continue;
        }
        let record: Value = decode(line)?;
        let Some(role @ ("user" | "assistant")) = record["type"].as_str() else {
            continue;
        };
        if [
            "isSidechain",
            "isMeta",
            "isCompactSummary",
            "isApiErrorMessage",
        ]
        .into_iter()
        .any(|flag| record[flag].as_bool() == Some(true))
        {
            continue;
        }
        let message = &record["message"];
        if record["sessionId"].as_str() != Some(session) || message["role"].as_str() != Some(role) {
            return Err("The Claude transcript does not match the session.");
        }
        let text = match &message["content"] {
            Value::String(text) => text.clone(),
            Value::Array(parts) if !parts.iter().any(|part| part["type"] == "tool_result") => parts
                .iter()
                .filter(|part| part["type"] == "text")
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            _ => continue,
        };
        if text.trim().is_empty()
            || (role == "user"
                && ["<command-name>", "<local-command-", "[Request interrupted"]
                    .iter()
                    .any(|prefix| text.trim_start().starts_with(prefix)))
        {
            continue;
        }
        let id = record["uuid"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or("The Claude transcript message identity is invalid.")?;
        if let Some(previous) = messages.iter().find(|entry| entry.id == id) {
            if previous.text != text {
                return Err("The Claude transcript repeats a conflicting message identity.");
            }
            continue;
        }
        if messages.len() == HISTORY_MESSAGES {
            messages.pop_front();
        }
        messages.push_back(HistoryMessage {
            id: id.into(),
            text,
        });
    }
    Ok(messages.into())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    pub enabled: bool,
    pub desktop_version: String,
    pub engine_version: String,
    pub process_image: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    pub session_id: String,
    pub pid: u32,
    pub proc_start: String,
    pub entrypoint: String,
    pub kind: String,
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub status: Option<SessionStatus>,
    pub version: String,
    pub pid_domain: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    Idle,
    Busy,
    Waiting,
}

#[derive(Debug, Deserialize)]
pub struct PermissionRequest {
    pub session_id: String,
    pub prompt_id: String,
    pub tool_name: String,
    pub tool_input: Map<String, Value>,
    hook_event_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckIn {
    pub session_id: String,
    pub at: u64,
    pub version: String,
    pub channel_version: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prompt {
    pub session_id: String,
    pub id: String,
    pub text: String,
    pub queued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PluginState {
    Taken,
    Started,
    Refused,
    Expired,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginReceipt {
    pub session_id: String,
    pub id: String,
    pub state: PluginState,
}

pub fn parse_config(bytes: &[u8]) -> Result<ClientConfig, &'static str> {
    let config: ClientConfig = decode(bytes)?;
    if !config.enabled {
        return Err("Claude Desktop Code is disabled.");
    }
    if config.desktop_version != DESKTOP_VERSION || config.engine_version != ENGINE_VERSION {
        return Err("This Claude Desktop Code version is unsupported.");
    }
    if config.process_image.trim().is_empty() {
        return Err("The Claude Code executable is unverified.");
    }
    Ok(config)
}

pub fn parse_registry(bytes: &[u8]) -> Result<Registry, &'static str> {
    let registry: Registry = decode(bytes)?;
    if !valid_id(&registry.session_id) || registry.pid == 0 || registry.proc_start.is_empty() {
        return Err("The Claude process registry identity is invalid.");
    }
    Ok(registry)
}

pub fn parse_hook(bytes: &[u8]) -> Result<PermissionRequest, &'static str> {
    let hook: PermissionRequest = decode(bytes)?;
    if hook.hook_event_name != "PermissionRequest"
        || !valid_id(&hook.session_id)
        || !valid_id(&hook.prompt_id)
        || !valid_id(&hook.tool_name)
    {
        return Err("The Claude permission request identity is invalid.");
    }
    Ok(hook)
}

pub fn hook_output(allow: bool) -> Value {
    let decision = if allow {
        json!({ "behavior": "allow" })
    } else {
        json!({ "behavior": "deny", "message": "Denied in Rooom." })
    };
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": decision
        }
    })
}

pub fn parse_check_in(bytes: &[u8]) -> Result<CheckIn, &'static str> {
    let check_in: CheckIn = decode(bytes)?;
    if !valid_id(&check_in.session_id)
        || check_in.channel_version != PLUGIN_VERSION
        || check_in.version != ENGINE_VERSION
    {
        return Err("The Claude plugin check-in has an unsupported identity or version.");
    }
    Ok(check_in)
}

pub fn parse_prompt(bytes: &[u8]) -> Result<Prompt, &'static str> {
    let prompt: Prompt = decode(bytes)?;
    if !valid_id(&prompt.session_id)
        || !valid_id(&prompt.id)
        || !valid_message(&prompt.text)
        || prompt.expires_at.checked_sub(prompt.queued_at) != Some(DELIVERY_LIFETIME_MS)
    {
        return Err("The Claude plugin prompt is invalid.");
    }
    Ok(prompt)
}

pub fn parse_receipt(bytes: &[u8]) -> Result<PluginReceipt, &'static str> {
    let receipt: PluginReceipt = decode(bytes)?;
    if !valid_id(&receipt.session_id) || !valid_id(&receipt.id) {
        return Err("The Claude plugin receipt identity is invalid.");
    }
    Ok(receipt)
}

pub fn valid_id(id: &str) -> bool {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_'))
    {
        return false;
    }
    let upper = id.to_ascii_uppercase();
    !matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit())
}

pub fn valid_message(text: &str) -> bool {
    let content = text.trim();
    !content.is_empty() && !content.starts_with('/') && text.len() <= RECORD_LIMIT / 8
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, &'static str> {
    if bytes.len() > RECORD_LIMIT {
        return Err("The Claude record exceeds the size limit.");
    }
    serde_json::from_slice(bytes).map_err(|_| "The Claude record has an invalid format.")
}
