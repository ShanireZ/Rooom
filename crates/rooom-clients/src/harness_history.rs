//! Text messages from the pinned Harness v4 compressed session format.

use serde_json::Value;
use std::{collections::BTreeMap, io::Read};

pub const MAX_COMPRESSED: usize = 8 * 1024 * 1024;
pub const MAX_DECODED: usize = 32 * 1024 * 1024;
const MAX_LINE: usize = 1024 * 1024;
const MAX_MESSAGES: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Message {
    pub id: String,
    pub text: String,
}

pub fn decode(raw: &[u8], session: &str) -> Result<Vec<Message>, &'static str> {
    if raw.is_empty() || raw.len() > MAX_COMPRESSED {
        return Err("Harness history exceeds the compressed size limit.");
    }
    let mut decoder = zstd::stream::read::Decoder::with_buffer(raw)
        .map_err(|_| "Harness history is not a supported Zstandard stream.")?;
    decoder
        .window_log_max(27)
        .map_err(|_| "The Harness history decoder could not be bounded.")?;
    let mut decoded = Vec::new();
    decoder
        .take(MAX_DECODED as u64 + 1)
        .read_to_end(&mut decoded)
        .map_err(|_| "Harness history is incomplete or corrupt.")?;
    if decoded.len() > MAX_DECODED {
        return Err("Harness history exceeds the decoded size limit.");
    }
    parse(&decoded, session)
}

fn line(raw: &[u8]) -> Result<Value, &'static str> {
    if raw.len() > MAX_LINE {
        return Err("A Harness history record exceeds the size limit.");
    }
    serde_json::from_slice(raw).map_err(|_| "Malformed Harness history record.")
}

fn parse(raw: &[u8], session: &str) -> Result<Vec<Message>, &'static str> {
    if !raw.ends_with(b"\n") {
        return Err("Harness history ends with an incomplete record.");
    }
    let mut lines = raw[..raw.len() - 1].split(|byte| *byte == b'\n');
    let header = line(lines.next().ok_or("Harness history has no header.")?)?;
    if header["type"] != "session"
        || header["version"] != 4
        || header["id"] != session
        || !header["parentSession"].is_null()
        || header["delegationDepth"] != 0
    {
        return Err("Harness history does not match the supported root session.");
    }
    let mut messages = Vec::<Message>::new();
    let mut identities = BTreeMap::new();
    for (seq, raw) in lines.enumerate() {
        let event = line(raw)?;
        if event["seq"].as_u64() != Some(seq as u64)
            || event["type"].as_str().is_none()
            || !event["data"].is_object()
        {
            return Err("Harness history has an invalid event sequence.");
        }
        let data = &event["data"];
        let message = match event["type"].as_str() {
            Some("user/message") if data["source"]["kind"] == "user" => data,
            Some("assistant/message") => &data["message"],
            _ => continue,
        };
        let id = message["id"]
            .as_str()
            .filter(|id| {
                !id.is_empty()
                    && id.len() <= 128
                    && id.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.')
                    })
            })
            .ok_or("A Harness history message has an invalid identity.")?;
        let text = text(&message["content"])?;
        if text.trim().is_empty() {
            continue;
        }
        let message = Message {
            id: id.into(),
            text,
        };
        if let Some(index) = identities.get(id).copied() {
            if messages[index] != message {
                return Err("Harness history repeats a message identity with different content.");
            }
        } else {
            identities.insert(id.to_owned(), messages.len());
            messages.push(message);
        }
    }
    let start = messages.len().saturating_sub(MAX_MESSAGES);
    Ok(messages.drain(start..).collect())
}

fn text(content: &Value) -> Result<String, &'static str> {
    if let Some(text) = content.as_str() {
        return Ok(text.into());
    }
    let blocks = content
        .as_array()
        .ok_or("A Harness history message has invalid content.")?;
    let mut words = Vec::new();
    for block in blocks {
        if block["type"] == "text" {
            words.push(
                block["text"]
                    .as_str()
                    .ok_or("A Harness history text block is invalid.")?,
            );
        }
    }
    Ok(words.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn header() -> Value {
        json!({"type":"session","version":4,"id":"session-synthetic","delegationDepth":0})
    }

    fn message(seq: u64, id: &str) -> Value {
        json!({"type":"assistant/message","seq":seq,"data":{"message":{
            "id":id,"content":[{"type":"reasoning","text":"private reasoning"},
                {"type":"text","text":"Synthetic reply"},{"type":"tool-call","name":"unrelated"}]
        }}})
    }

    fn frame(rows: &[Value]) -> Vec<u8> {
        let text = rows
            .iter()
            .map(|row| format!("{row}\n"))
            .collect::<String>();
        zstd::stream::encode_all(text.as_bytes(), 1).unwrap()
    }

    #[test]
    fn native_concatenated_frames_keep_message_order_and_exact_identity() {
        let mut raw = frame(&[header()]);
        raw.extend(frame(&[
            json!({"type":"user/message","seq":0,"data":{"id":"u-1",
                "source":{"kind":"user"},"content":[{"type":"text","text":"Synthetic prompt"}]}}),
            message(1, "a-1"),
            json!({"type":"user/message","seq":2,"data":{"id":"runtime",
                "source":{"kind":"time-context"},"content":[{"type":"text","text":"hidden"}]}}),
            message(3, "a-1"),
        ]));
        let messages = decode(&raw, "session-synthetic").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, "u-1");
        assert_eq!(messages[1].text, "Synthetic reply");
        assert!(decode(&raw, "other-session").is_err());
        raw.pop();
        assert!(decode(&raw, "session-synthetic").is_err());
    }

    #[test]
    fn refuses_wrong_versions_sequences_and_conflicting_message_ids() {
        let mut wrong = header();
        wrong["version"] = json!(5);
        assert!(decode(&frame(&[wrong]), "session-synthetic").is_err());
        assert!(decode(&frame(&[header(), message(1, "a")]), "session-synthetic").is_err());
        let mut conflict = message(1, "a");
        conflict["data"]["message"]["content"] = json!([{"type":"text","text":"different"}]);
        assert!(
            decode(
                &frame(&[header(), message(0, "a"), conflict]),
                "session-synthetic"
            )
            .is_err()
        );
        let mut child = header();
        child["parentSession"] = json!("parent");
        assert!(decode(&frame(&[child]), "session-synthetic").is_err());
    }

    #[test]
    fn bounds_decoded_records_and_rejects_torn_json() {
        let expanded = zstd::stream::encode_all(&vec![b' '; MAX_DECODED + 1][..], 1).unwrap();
        assert!(decode(&expanded, "session-synthetic").is_err());
        let long = vec![b' '; MAX_LINE + 1];
        assert!(line(&long).is_err());
        let torn = zstd::stream::encode_all(b"{".as_slice(), 1).unwrap();
        assert!(decode(&torn, "session-synthetic").is_err());
    }
}
