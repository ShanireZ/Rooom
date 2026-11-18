//! The bounded subset of Codex rollout, approval and queue formats used by
//! the Desktop adapters. This module performs no I/O.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, VecDeque};

pub const DESKTOP_VERSION: &str = "26.1111.2228.0";
pub const ENGINE_VERSION: &str = "0.169.0-alpha.11.1";
pub const COMMAND_REQUEST: &str = "item/commandExecution/requestApproval";
pub const COMMAND_DECISION: &str = "thread-follower-command-approval-decision";
pub const MAX_DOCUMENT_BYTES: usize = 128 * 1024;
pub const HISTORY_WINDOW: usize = 4 * 1024 * 1024;
const HISTORY_MESSAGES: usize = 256;

pub fn identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() <= 96
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct Thread {
    pub id: String,
    pub originator: String,
    pub source: String,
    pub cli_version: String,
    #[serde(skip)]
    pub cwd: Option<String>,
    history_mode: Option<String>,
}

impl Thread {
    pub fn desktop_origin(&self) -> bool {
        matches!(
            self.originator.as_str(),
            "Codex Desktop" | "codex_desktop" | "codex_work_desktop"
        ) && !matches!(self.source.as_str(), "cli" | "exec" | "terminal")
    }
}

#[derive(Debug, Deserialize)]
struct RolloutLine {
    #[serde(rename = "type")]
    kind: String,
    payload: Value,
}

#[derive(Debug, Eq, PartialEq)]
pub struct HistoryMessage {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Rollout {
    pub thread: Thread,
    pub messages: Vec<HistoryMessage>,
}

#[cfg(test)]
#[test]
fn native_receipt_requires_the_exact_input_and_its_own_new_turn() {
    let unrelated = r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"older-turn"}}
{"type":"event_msg","payload":{"type":"user_message","message":"Another prompt"}}
{"type":"event_msg","payload":{"type":"task_complete","turn_id":"older-turn"}}"#;
    let matching = r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"matching-turn"}}
{"type":"turn_context","payload":{"turn_id":"matching-turn"}}
{"type":"event_msg","payload":{"type":"user_message","message":"Rooom synthetic message"}}"#;
    let missing_start = r#"{"type":"event_msg","payload":{"type":"user_message","message":"Rooom synthetic message"}}"#;
    assert_eq!(
        native_delivery_evidence(unrelated, "thread-1", "Rooom synthetic message"),
        Ok(None)
    );
    assert_eq!(
        native_delivery_evidence(missing_start, "thread-1", "Rooom synthetic message"),
        Ok(None)
    );
    let events = format!("{unrelated}\n{matching}");
    assert_eq!(
        native_delivery_evidence(&events, "thread-1", "Rooom synthetic message"),
        Ok(Some("matching-turn".into()))
    );
    let duplicate = format!("{events}\n{missing_start}");
    assert_eq!(
        native_delivery_evidence(&duplicate, "thread-1", "Rooom synthetic message"),
        Ok(None)
    );
}

#[cfg(test)]
#[test]
fn desktop_user_message_item_must_match_thread_turn_and_text() {
    let input = r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1"}}
{"type":"event_msg","payload":{"type":"item_completed","thread_id":"thread-1","turn_id":"turn-1","item":{"type":"UserMessage","id":"item-1","content":[{"type":"text","text":"Synthetic Desktop input"}]}}}"#;
    assert_eq!(
        native_delivery_evidence(input, "thread-1", "Synthetic Desktop input"),
        Ok(Some("turn-1".into()))
    );
    assert_eq!(
        native_delivery_evidence(input, "another-thread", "Synthetic Desktop input"),
        Ok(None)
    );
    let wrong_turn = input.replace(
        "\"turn_id\":\"turn-1\",\"item\"",
        "\"turn_id\":\"turn-2\",\"item\"",
    );
    assert_eq!(
        native_delivery_evidence(&wrong_turn, "thread-1", "Synthetic Desktop input"),
        Ok(None)
    );
}

pub fn parse_rollout(input: &str) -> Result<Rollout, &'static str> {
    validate_size(input)?;
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return Err("Codex rollout is empty");
    }
    let header_end = trimmed.find('\n').map_or(trimmed.len(), |index| index + 1);
    let (header, records) = trimmed.split_at(header_end);
    let thread = parse_thread_metadata(header)?;
    let start = (input.len() - records.len()) as u64;
    let messages = project_messages(records, start, &thread)?;
    Ok(Rollout { thread, messages })
}

pub fn parse_thread_metadata(line: &str) -> Result<Thread, &'static str> {
    validate_size(line)?;
    let first = parse_line(line)?;
    if first.kind != "session_meta" {
        return Err("Codex rollout lacks session metadata");
    }
    let cwd = first.payload["cwd"].as_str().map(str::to_owned);
    let mut thread: Thread =
        serde_json::from_value(first.payload).map_err(|_| "Invalid Codex session metadata")?;
    if !identifier(&thread.id) || thread.source.trim().is_empty() {
        return Err("Invalid Codex session metadata");
    }
    thread.cwd = cwd;
    Ok(thread)
}

/// Read complete records in a bounded window whose header has been verified.
/// `start` is the first record's absolute byte offset in the rollout.
pub fn parse_history(
    input: &[u8],
    start: u64,
    thread: &Thread,
) -> Result<Vec<HistoryMessage>, &'static str> {
    if input.len() > HISTORY_WINDOW || start.checked_add(input.len() as u64).is_none() {
        return Err("Codex history exceeds its size limit");
    }
    let end = input
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |i| i + 1);
    let complete = std::str::from_utf8(&input[..end]).map_err(|_| "Invalid Codex history text")?;
    project_messages(complete, start, thread)
}

fn project_messages(
    records: &str,
    start: u64,
    thread: &Thread,
) -> Result<Vec<HistoryMessage>, &'static str> {
    let mut messages = VecDeque::new();
    let mut seen = BTreeSet::new();
    let mut offset = start;
    for record in records.split_inclusive('\n') {
        let position = offset;
        offset += record.len() as u64;
        if record.trim().is_empty() {
            continue;
        }
        let line = parse_line(record)?;
        if let Some(message) = history_message(&line, thread, position)?
            && seen.insert(message.id.clone())
        {
            if messages.len() == HISTORY_MESSAGES {
                messages.pop_front();
            }
            messages.push_back(message);
        }
    }
    Ok(messages.into())
}

fn history_message(
    line: &RolloutLine,
    thread: &Thread,
    offset: u64,
) -> Result<Option<HistoryMessage>, &'static str> {
    if line.kind == "session_meta" {
        return Err("Codex rollout has repeated session metadata");
    }
    let payload = &line.payload;
    if line.kind == "response_item"
        && payload["type"] == "message"
        && payload["role"] == "assistant"
    {
        let text = message_text(payload, "output_text")?;
        let id = payload["id"]
            .as_str()
            .filter(|id| identifier(id))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("offset:{offset}"));
        return Ok(history_entry(id, text));
    }

    // Paginated rollouts keep accepted user input as a separate turn item.
    // User-role response items also contain client-generated context.
    let item = &payload["item"];
    if thread.history_mode.as_deref() == Some("paginated")
        && line.kind == "event_msg"
        && payload["type"] == "item_completed"
        && payload["thread_id"].as_str() == Some(thread.id.as_str())
        && payload["turn_id"].as_str().is_some_and(identifier)
        && item["type"] == "UserMessage"
        && let Some(id) = item["id"].as_str().filter(|id| identifier(id))
    {
        return Ok(history_entry(
            format!("input:{id}"),
            message_text(item, "text")?,
        ));
    }
    Ok(None)
}

fn message_text(message: &Value, part_type: &str) -> Result<String, &'static str> {
    let content = message["content"]
        .as_array()
        .ok_or("Invalid Codex history message")?;
    Ok(content
        .iter()
        .filter(|part| part["type"] == part_type)
        .map(|part| part["text"].as_str().ok_or("Invalid Codex history text"))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n"))
}

fn history_entry(id: String, text: String) -> Option<HistoryMessage> {
    (!text.trim().is_empty()).then_some(HistoryMessage { id, text })
}

pub fn native_delivery_evidence(
    input: &str,
    thread: &str,
    text: &str,
) -> Result<Option<String>, &'static str> {
    let mut active = None;
    let mut matched_event = None;
    let mut matched_item = None;
    for line in input.lines().filter(|line| !line.trim().is_empty()) {
        let event = parse_line(line)?;
        if event.kind != "event_msg" {
            continue;
        }
        let payload = &event.payload;
        match payload["type"].as_str() {
            Some("task_started") => {
                active = payload["turn_id"]
                    .as_str()
                    .filter(|id| identifier(id))
                    .map(str::to_owned);
            }
            Some("task_complete" | "turn_aborted") => active = None,
            Some("user_message") if payload["message"].as_str() == Some(text) => {
                if matched_event.is_some() || active.is_none() {
                    return Ok(None);
                }
                matched_event = active.clone();
            }
            Some("item_completed") if payload["item"]["type"] == "UserMessage" => {
                let content = payload["item"]["content"].as_array();
                let same_input = content.is_some_and(|parts| {
                    matches!(parts.as_slice(), [part] if part["type"] == "text" && part["text"].as_str() == Some(text))
                });
                if same_input
                    && payload["thread_id"].as_str() == Some(thread)
                    && active
                        .as_deref()
                        .is_some_and(|turn| payload["turn_id"] == turn)
                {
                    if matched_item.is_some() {
                        return Ok(None);
                    }
                    matched_item = active.clone();
                }
            }
            _ => {}
        }
    }
    if matched_event.is_some() && matched_item.is_some() && matched_event != matched_item {
        return Ok(None);
    }
    Ok(matched_item.or(matched_event))
}

fn parse_line(line: &str) -> Result<RolloutLine, &'static str> {
    serde_json::from_str(line).map_err(|_| "Invalid Codex rollout event")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Owner {
    pub enabled: bool,
    pub desktop_version: String,
    pub engine_version: String,
    pub handled_by_client_id: Option<String>,
    pub conversation_id: String,
    pub live_request: Option<Value>,
}

pub fn parse_owner(input: &str) -> Result<Owner, &'static str> {
    validate_size(input)?;
    let owner: Owner = serde_json::from_str(input).map_err(|_| "Invalid Codex owner fixture")?;
    if !identifier(&owner.conversation_id)
        || owner
            .handled_by_client_id
            .as_deref()
            .is_some_and(|id| !identifier(id))
    {
        return Err("Invalid Codex owner identity");
    }
    Ok(owner)
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Approval {
    pub handled_by_client_id: String,
    pub request: Value,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct CommandRequest {
    pub id: String,
    pub method: String,
    pub params: CommandParams,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandParams {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub command: String,
    pub available_decisions: Vec<String>,
}

impl Approval {
    pub fn command(&self) -> Result<CommandRequest, &'static str> {
        let request: CommandRequest = serde_json::from_value(self.request.clone())
            .map_err(|_| "Invalid Codex command request")?;
        if request.method != COMMAND_REQUEST
            || !identifier(&self.handled_by_client_id)
            || !identifier(&request.id)
            || !identifier(&request.params.thread_id)
            || !identifier(&request.params.turn_id)
            || !identifier(&request.params.item_id)
            || request.params.command.trim().is_empty()
        {
            return Err("Invalid Codex command request");
        }
        Ok(request)
    }
}

pub fn parse_approval(input: &str) -> Result<Approval, &'static str> {
    validate_size(input)?;
    let approval: Approval =
        serde_json::from_str(input).map_err(|_| "Invalid Codex approval envelope")?;
    approval.command()?;
    Ok(approval)
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueueReceipt {
    pub message_id: String,
    pub thread_id: String,
}

impl QueueReceipt {
    pub fn parse_stdout(input: &str) -> Result<Self, &'static str> {
        let (message_id, thread_id) = input
            .trim()
            .strip_prefix("Queued message ")
            .and_then(|value| value.strip_suffix('.'))
            .and_then(|value| value.split_once(" for thread "))
            .ok_or("Invalid Codex queue receipt")?;
        if !identifier(message_id) || !identifier(thread_id) {
            return Err("Invalid Codex queue receipt identity");
        }
        Ok(Self {
            message_id: message_id.into(),
            thread_id: thread_id.into(),
        })
    }

    pub fn parse_json(input: &str) -> Result<Self, &'static str> {
        validate_size(input)?;
        let receipt: Self =
            serde_json::from_str(input).map_err(|_| "Invalid Codex queue fixture")?;
        if !identifier(&receipt.message_id) || !identifier(&receipt.thread_id) {
            return Err("Invalid Codex queue receipt identity");
        }
        Ok(receipt)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryEventKind {
    QueueTaken,
    TaskStarted,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryEvent {
    #[serde(rename = "type")]
    pub kind: DeliveryEventKind,
    pub thread_id: String,
    pub queued_submission_id: String,
    pub turn_id: Option<String>,
}

pub fn parse_delivery_events(input: &str) -> Result<Vec<DeliveryEvent>, &'static str> {
    validate_size(input)?;
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let event: DeliveryEvent =
                serde_json::from_str(line).map_err(|_| "Invalid Codex delivery event")?;
            if !identifier(&event.thread_id)
                || !identifier(&event.queued_submission_id)
                || (event.kind == DeliveryEventKind::TaskStarted
                    && !event.turn_id.as_deref().is_some_and(identifier))
            {
                return Err("Invalid Codex delivery event identity");
            }
            Ok(event)
        })
        .collect()
}

fn validate_size(input: &str) -> Result<(), &'static str> {
    if input.len() > MAX_DOCUMENT_BYTES {
        return Err("Codex fixture exceeds its size limit");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREAD: &str = include_str!("../../../tests/fixtures/codex/thread.jsonl");
    const APPROVAL: &str = include_str!("../../../tests/fixtures/codex/approval.json");
    const OWNER: &str = include_str!("../../../tests/fixtures/codex/owner.json");

    #[test]
    fn parses_the_pinned_thread_command_and_queue_receipt() {
        let rollout = parse_rollout(THREAD).unwrap();
        assert!(rollout.thread.desktop_origin());
        assert_eq!(rollout.thread.cli_version, ENGINE_VERSION);
        assert_eq!(rollout.messages.last().unwrap().id, "item-intro");
        let owner = parse_owner(OWNER).unwrap();
        assert_eq!(owner.desktop_version, DESKTOP_VERSION);
        assert_eq!(owner.engine_version, ENGINE_VERSION);
        let approval = parse_approval(APPROVAL).unwrap();
        let command = approval.command().unwrap();
        assert_eq!(owner.live_request, Some(approval.request));
        assert_eq!(command.params.thread_id, rollout.thread.id);
        assert_eq!(command.id, "request-1");
        let queue =
            QueueReceipt::parse_stdout("Queued message queue-1 for thread session-1.\r\n").unwrap();
        assert_eq!(queue.message_id, "queue-1");
        assert_eq!(queue.thread_id, command.params.thread_id);
    }

    #[test]
    fn rejects_missing_or_malformed_wire_fields() {
        assert!(parse_rollout(&format!("{THREAD}\n{{")).is_err());
        assert!(parse_approval(&APPROVAL.replace("threadId", "changed")).is_err());
        assert!(
            QueueReceipt::parse_stdout("Queued message queue-1 for thread ../outside.").is_err()
        );
        assert!(
            QueueReceipt::parse_stdout("Queued message queue-1 for thread session-1.\nExtra")
                .is_err()
        );
        assert!(parse_delivery_events(
            r#"{"type":"task_started","thread_id":"session-1","queued_submission_id":"queue-1"}"#
        ).is_err());
    }

    #[test]
    fn history_uses_complete_assistant_records_and_stable_ids() {
        let thread = parse_thread_metadata(THREAD.lines().next().unwrap()).unwrap();
        let message = r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Synthetic reply ✓"}]}}"#;
        let named = message.replace("\"role\"", "\"id\":\"reply-1\",\"role\"");
        let user_context = message.replace("\"assistant\"", "\"user\"");
        let event = r#"{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"Repeat of reply"}}"#;
        let excerpt = format!("{user_context}\r\n{named}\n{named}\n{event}\n{message}\n{{");
        let start = 4096;
        let messages = parse_history(excerpt.as_bytes(), start, &thread).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, "reply-1");
        assert_eq!(messages[1].text, "Synthetic reply ✓");

        let position = excerpt.rfind(message).unwrap();
        let later_window = parse_history(
            &excerpt.as_bytes()[position..],
            start + position as u64,
            &thread,
        )
        .unwrap();
        assert_eq!(later_window, messages[1..]);
        assert!(parse_history(format!("{excerpt}\n").as_bytes(), start, &thread).is_err());
    }

    #[test]
    fn history_retains_the_latest_256_replies() {
        let thread = parse_thread_metadata(THREAD.lines().next().unwrap()).unwrap();
        let mut excerpt = String::new();
        for index in 0..300 {
            use std::fmt::Write as _;
            writeln!(
                excerpt,
                r#"{{"type":"response_item","payload":{{"type":"message","id":"reply-{index}","role":"assistant","content":[{{"type":"output_text","text":"Reply {index}"}}]}}}}"#
            )
            .unwrap();
        }
        let messages = parse_history(excerpt.as_bytes(), 0, &thread).unwrap();
        assert_eq!(messages.len(), 256);
        assert_eq!(messages.first().unwrap().id, "reply-44");
        assert_eq!(messages.last().unwrap().id, "reply-299");
    }

    #[test]
    fn paginated_history_checks_input_identity_and_ignores_non_text_parts() {
        let (header, records) = THREAD.split_once('\n').unwrap();
        let thread = parse_thread_metadata(header).unwrap();
        let input = r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"session-1","turn_id":"turn-2","item":{"type":"UserMessage","id":"item-second-input","content":[{"type":"text","text":"First part"},{"type":"image","url":"fixture-image"},{"type":"text","text":"Second part"}]}}}"#;
        let foreign = input.replace("session-1", "foreign-session");
        let no_turn = input.replace("turn-2", "../invalid");
        let invalid = format!("{records}{foreign}\n{no_turn}\n");
        let accepted = format!("{invalid}{input}\n{input}\n");
        let messages = parse_history(accepted.as_bytes(), 0, &thread).unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2].id, "input:item-second-input");
        assert_eq!(messages[2].text, "First part\nSecond part");
        let invalid = parse_history(invalid.as_bytes(), 0, &thread).unwrap();
        assert_eq!(invalid, messages[..2]);

        for header in [
            header.replace("paginated", "legacy"),
            header.replace("paginated", "unknown"),
            header.replace(",\"history_mode\":\"paginated\"", ""),
        ] {
            let thread = parse_thread_metadata(&header).unwrap();
            let messages = parse_history(accepted.as_bytes(), 0, &thread).unwrap();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].id, "item-intro");
        }
    }
}
