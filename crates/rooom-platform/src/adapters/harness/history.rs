use rooom_clients::harness_history::{self, Message};
use rooom_core::inbox::{Client, Session, SessionKey, TimelineEntry};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

const UNAVAILABLE: &str =
    "Harness history is unavailable or incomplete. The live channel remains available.";

#[derive(Default)]
pub struct HistoryTracker {
    entries: BTreeMap<String, Cached>,
}

struct Cached {
    path: PathBuf,
    stamp: (u64, u64, u64),
    messages: Vec<Message>,
}

fn directory(path: &Path) -> Result<(), &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| UNAVAILABLE)?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(UNAVAILABLE);
    }
    Ok(())
}

fn locate(root: &Path, session: &str) -> Result<PathBuf, &'static str> {
    directory(root)?;
    let mut found = None;
    for (count, entry) in fs::read_dir(root).map_err(|_| UNAVAILABLE)?.enumerate() {
        if count >= 1024 {
            return Err(UNAVAILABLE);
        }
        let project = entry.map_err(|_| UNAVAILABLE)?.path();
        if directory(&project).is_err() {
            continue;
        }
        let folder = project.join(session);
        if !folder.try_exists().map_err(|_| UNAVAILABLE)? {
            continue;
        }
        directory(&folder)?;
        for item in fs::read_dir(&folder).map_err(|_| UNAVAILABLE)? {
            let name = item.map_err(|_| UNAVAILABLE)?.file_name();
            if let Some(version) = name
                .to_str()
                .and_then(|name| name.strip_prefix("session.v"))
                .and_then(|name| name.strip_suffix(".jsonl.zstd"))
                .and_then(|version| version.parse::<u64>().ok())
                && version > 4
            {
                return Err("Unsupported Harness history generation.");
            }
        }
        let path = folder.join("session.v4.jsonl.zstd");
        if found.replace(path).is_some() {
            return Err("Harness history matches more than one project.");
        }
    }
    found.ok_or(UNAVAILABLE)
}

fn stamp(metadata: &fs::Metadata) -> (u64, u64, u64) {
    (
        metadata.file_size(),
        metadata.creation_time(),
        metadata.last_write_time(),
    )
}

impl HistoryTracker {
    pub fn read(&mut self, root: &Path, session: &str) -> Result<&[Message], &'static str> {
        let path = locate(root, session)?;
        let metadata = fs::symlink_metadata(&path).map_err(|_| UNAVAILABLE)?;
        if !metadata.is_file()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || metadata.len() > harness_history::MAX_COMPRESSED as u64
        {
            return Err(UNAVAILABLE);
        }
        let observed = stamp(&metadata);
        if !self
            .entries
            .get(session)
            .is_some_and(|entry| entry.path == path && entry.stamp == observed)
        {
            let mut bytes = Vec::new();
            fs::File::open(&path)
                .map_err(|_| UNAVAILABLE)?
                .take(harness_history::MAX_COMPRESSED as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| UNAVAILABLE)?;
            let messages = harness_history::decode(&bytes, session)?;
            if stamp(&fs::metadata(&path).map_err(|_| UNAVAILABLE)?) != observed {
                return Err(UNAVAILABLE);
            }
            self.entries.insert(
                session.into(),
                Cached {
                    path,
                    stamp: observed,
                    messages,
                },
            );
        }
        Ok(&self.entries[session].messages)
    }

    pub fn retain(&mut self, sessions: &[Session]) {
        self.entries
            .retain(|id, _| sessions.iter().any(|session| session.key.id == *id));
    }

    pub fn project(
        &mut self,
        root: &Path,
        session: &str,
    ) -> (Vec<TimelineEntry>, Option<&'static str>) {
        if let Err(error) = self.read(root, session) {
            self.entries.remove(session);
            return (Vec::new(), Some(error));
        }
        let entries = self.entries[session]
            .messages
            .iter()
            .map(|message| TimelineEntry {
                session: SessionKey {
                    client: Client::Harness,
                    id: session.into(),
                },
                id: message.id.clone(),
                text: message.text.clone(),
            })
            .collect();
        (entries, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn reads_only_the_matching_generation_and_reloads_changed_files() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "rooom-m3-harness-history-20261115-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let fixture = Fixture(root);
        let session = "session-history-fixture";
        let folder = fixture.0.join("project").join(session);
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("session.v4.jsonl.zstd");
        let raw = include_bytes!("../../../../../tests/fixtures/harness/history.jsonl.zstd");
        fs::write(&path, raw).unwrap();
        let mut tracker = HistoryTracker::default();
        let expected = tracker.read(&fixture.0, session).unwrap().to_vec();
        assert_eq!(expected.len(), 2);
        assert_eq!(tracker.read(&fixture.0, session).unwrap(), expected);
        fs::write(&path, &raw[..raw.len() - 1]).unwrap();
        let (unavailable, warning) = tracker.project(&fixture.0, session);
        assert!(warning.is_some());
        assert!(unavailable.is_empty());
        assert_eq!(fs::read(&path).unwrap(), &raw[..raw.len() - 1]);
        fs::write(&path, raw).unwrap();
        assert_eq!(tracker.read(&fixture.0, session).unwrap(), expected);
        let other_root = fixture.0.join("other-root");
        fs::create_dir(&other_root).unwrap();
        let (unrelated, warning) = tracker.project(&other_root, session);
        assert!(warning.is_some());
        assert!(unrelated.is_empty());
        assert!(!tracker.entries.contains_key(session));
        assert_eq!(tracker.read(&fixture.0, session).unwrap(), expected);
        let foreign = fixture.0.join("project/foreign");
        fs::create_dir_all(&foreign).unwrap();
        fs::write(foreign.join("session.v4.jsonl.zstd"), raw).unwrap();
        assert!(tracker.read(&fixture.0, "foreign").is_err());
        fs::write(folder.join("session.v5.jsonl.zstd"), raw).unwrap();
        assert!(tracker.read(&fixture.0, session).is_err());
        tracker.retain(&[]);
        assert!(tracker.entries.is_empty());
    }
}
