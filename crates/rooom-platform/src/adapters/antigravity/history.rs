use crate::adapters::{Feed, windows};
use rooom_clients::antigravity::{self, HISTORY_WINDOW};
use rooom_core::inbox::{SessionKey, TimelineEntry};
use std::{
    collections::BTreeMap,
    fs, io,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

const UNAVAILABLE: &str =
    "Antigravity history is unavailable or incomplete. The live channel remains available.";

#[derive(Default)]
pub(super) struct Tracker {
    cached: BTreeMap<String, Projection>,
}

struct Projection {
    path: PathBuf,
    stamp: (u64, u64, u64),
    messages: Vec<TimelineEntry>,
}

impl Tracker {
    pub(super) fn append(&mut self, root: &Path, feed: &mut Feed) {
        self.cached
            .retain(|id, _| feed.sessions.iter().any(|session| session.key.id == *id));
        for session in &feed.sessions {
            match self.read(root, &session.key) {
                Ok(Some(messages)) => feed.timeline.extend_from_slice(messages),
                Ok(None) => {
                    self.cached.remove(&session.key.id);
                }
                Err(_) => {
                    self.cached.remove(&session.key.id);
                    feed.warning = Some(UNAVAILABLE);
                }
            }
        }
    }

    fn read(&mut self, root: &Path, session: &SessionKey) -> io::Result<Option<&[TimelineEntry]>> {
        let Some(path) = transcript_path(root, &session.id)? else {
            return Ok(None);
        };
        let (mut file, metadata) = match windows::open_plain_file(&path) {
            Ok(opened) => opened,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let stamp = windows::file_stamp(&metadata);
        if !self
            .cached
            .get(&session.id)
            .is_some_and(|cached| cached.path == path && cached.stamp == stamp)
        {
            let (_, bytes) = windows::jsonl_tail(&mut file, stamp, 0, HISTORY_WINDOW)?;
            let messages = antigravity::parse_history(&bytes)
                .map_err(|_| io::ErrorKind::InvalidData)?
                .into_iter()
                .map(|message| TimelineEntry {
                    session: session.clone(),
                    id: format!("step-{}", message.step),
                    text: message.text,
                })
                .collect();
            self.cached.insert(
                session.id.clone(),
                Projection {
                    path,
                    stamp,
                    messages,
                },
            );
        }
        Ok(Some(&self.cached[&session.id].messages))
    }
}

fn transcript_path(root: &Path, id: &str) -> io::Result<Option<PathBuf>> {
    if !antigravity::valid_id(id) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut path = root.to_owned();
    for component in ["brain", id, ".system_generated", "logs", "transcript.jsonl"] {
        match fs::symlink_metadata(&path) {
            Ok(metadata)
                if metadata.is_dir()
                    && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
            _ => return Err(io::ErrorKind::InvalidData.into()),
        }
        path.push(component);
    }
    Ok(Some(path))
}
