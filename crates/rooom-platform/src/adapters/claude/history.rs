use rooom_clients::claude::{self, HISTORY_WINDOW};
use rooom_core::inbox::{SessionKey, TimelineEntry};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

const UNAVAILABLE: &str =
    "Claude history is unavailable or incomplete. The live channel remains available.";

#[derive(Default)]
pub(in crate::adapters) struct Tracker {
    sessions: BTreeMap<SessionKey, Cached>,
}

struct Cached {
    file: PathBuf,
    stamp: (u64, u64, u64),
    messages: Vec<TimelineEntry>,
}

impl Tracker {
    pub(super) fn append(&mut self, registry: &Path, feed: &mut super::Feed) {
        self.sessions
            .retain(|key, _| feed.sessions.iter().any(|session| session.key == *key));
        if feed.sessions.is_empty() {
            return;
        }
        let Some(parent) = registry.parent() else {
            return;
        };
        let Ok(projects) = project_folders(&parent.join("projects")) else {
            self.sessions.clear();
            feed.warning = Some(UNAVAILABLE);
            return;
        };
        for session in &feed.sessions {
            match self.timeline(&projects, &session.key) {
                Ok(true) => feed
                    .timeline
                    .extend_from_slice(&self.sessions[&session.key].messages),
                Ok(false) => {
                    self.sessions.remove(&session.key);
                }
                Err(_) => {
                    self.sessions.remove(&session.key);
                    feed.warning = Some(UNAVAILABLE);
                }
            }
        }
    }

    fn timeline(&mut self, projects: &[PathBuf], key: &SessionKey) -> io::Result<bool> {
        let Some(path) = locate(projects, &key.id)? else {
            return Ok(false);
        };
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || super::reparse(&metadata) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let stamp = stamp_of(&metadata);
        if self
            .sessions
            .get(key)
            .is_some_and(|cached| cached.file == path && cached.stamp == stamp)
        {
            return Ok(true);
        }

        // Start at a whole JSONL record, even if the window cuts a long line.
        let len = metadata.len();
        let start = len.saturating_sub(HISTORY_WINDOW as u64);
        let preceding = u64::from(start > 0);
        file.seek(SeekFrom::Start(start - preceding))?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(len - start + preceding)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != len - start + preceding || stamp_of(&file.metadata()?) != stamp {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let first = if preceding == 0 {
            0
        } else {
            bytes
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |index| index + 1)
        };
        let messages = claude::parse_history(&bytes[first..], &key.id)
            .map_err(|_| io::ErrorKind::InvalidData)?
            .into_iter()
            .map(|message| TimelineEntry {
                session: key.clone(),
                id: message.id,
                text: message.text,
            })
            .collect();
        self.sessions.insert(
            key.clone(),
            Cached {
                file: path,
                stamp,
                messages,
            },
        );
        Ok(true)
    }
}

fn project_folders(root: &Path) -> io::Result<Vec<PathBuf>> {
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Ok(info) if info.is_dir() && !super::reparse(&info) => (),
        _ => return Err(io::ErrorKind::InvalidData.into()),
    }
    let mut folders = Vec::new();
    for (index, entry) in fs::read_dir(root)?.enumerate() {
        if index >= 1024 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let entry = entry?;
        if entry.file_type()?.is_dir() && super::safe_dir(&entry.path()) {
            folders.push(entry.path());
        }
    }
    Ok(folders)
}

fn locate(projects: &[PathBuf], session: &str) -> io::Result<Option<PathBuf>> {
    let name = format!("{session}.jsonl");
    let mut result = None;
    for folder in projects {
        let path = folder.join(&name);
        match fs::symlink_metadata(&path) {
            Ok(info) if info.is_file() && !super::reparse(&info) && result.is_none() => {
                result = Some(path)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            _ => return Err(io::ErrorKind::InvalidData.into()),
        }
    }
    Ok(result)
}

fn stamp_of(metadata: &fs::Metadata) -> (u64, u64, u64) {
    (
        metadata.file_size(),
        metadata.creation_time(),
        metadata.last_write_time(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::Feed;
    use rooom_core::inbox::{Client, Session};

    #[test]
    fn reads_large_transcript_tail_and_quarantines_ambiguous_sources() {
        let fixture = super::super::tests::Fixture::new();
        let registry = fixture.0.join("sessions");
        let project = fixture.0.join("projects/selected");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir(&registry).unwrap();
        let source = project.join("session-1.jsonl");
        let mut bytes = vec![b' '; HISTORY_WINDOW + 1];
        bytes.push(b'\n');
        bytes.extend_from_slice(include_bytes!(
            "../../../../../tests/fixtures/claude/history.jsonl"
        ));
        fs::write(&source, &bytes).unwrap();
        let mut feed = Feed {
            sessions: vec![Session {
                key: SessionKey {
                    client: Client::Claude,
                    id: "session-1".into(),
                },
                title: "Verified session".into(),
                send_blocked: None,
            }],
            ..Feed::default()
        };
        let mut history = Tracker::default();
        history.append(&registry, &mut feed);
        assert_eq!(feed.timeline.len(), 2);
        assert!(feed.warning.is_none());

        let foreign = project.with_file_name("foreign");
        fs::create_dir(&foreign).unwrap();
        let conflicting = foreign.join("session-1.jsonl");
        fs::write(&conflicting, &bytes).unwrap();
        feed.timeline.clear();
        history.append(&registry, &mut feed);
        assert!(feed.timeline.is_empty());
        assert_eq!(feed.warning, Some(UNAVAILABLE));
        assert_eq!(feed.sessions.len(), 1);
        assert!(history.sessions.is_empty());

        fs::remove_file(&conflicting).unwrap();
        feed.warning = None;
        history.append(&registry, &mut feed);
        assert_eq!(feed.timeline.len(), 2);
        assert!(feed.warning.is_none());
        assert_eq!(fs::read(source).unwrap(), bytes);
    }
}
