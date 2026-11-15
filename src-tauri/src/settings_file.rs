use rooom_core::{Settings, SettingsIssue};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> (Settings, Option<SettingsIssue>) {
        let existing = fs::read_to_string(&self.path);
        if existing
            .as_ref()
            .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
        {
            let settings = Settings::default();
            match self.save(&settings, false) {
                Ok(()) => return (settings, None),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    return Self::read(fs::read_to_string(&self.path));
                }
                Err(_) => return (settings, Some(SettingsIssue::Unwritable)),
            }
        }
        Self::read(existing)
    }

    fn read(existing: io::Result<String>) -> (Settings, Option<SettingsIssue>) {
        match existing {
            Ok(text) => match Settings::from_json(&text) {
                Ok(settings) => (settings, None),
                Err(_) => (Settings::default(), Some(SettingsIssue::Invalid)),
            },
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                (Settings::default(), Some(SettingsIssue::Invalid))
            }
            Err(_) => (Settings::default(), Some(SettingsIssue::Unreadable)),
        }
    }

    fn save(&self, settings: &Settings, replace: bool) -> io::Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| io::Error::other("Settings need a parent directory"))?;
        fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec_pretty(settings).map_err(io::Error::other)?;
        let (temporary, mut file) = loop {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temporary = parent.join(format!(".settings-{}-{sequence}.tmp", std::process::id()));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            {
                Ok(file) => break (temporary, file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        let written = file.write_all(&bytes).and_then(|_| file.sync_all());
        drop(file);
        let result = written.and_then(|_| {
            let from: Vec<_> = temporary.as_os_str().encode_wide().chain([0]).collect();
            let to: Vec<_> = self.path.as_os_str().encode_wide().chain([0]).collect();
            let mut flags = MOVEFILE_WRITE_THROUGH;
            if replace {
                flags |= MOVEFILE_REPLACE_EXISTING;
            }
            if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rooom_core::Edge;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "rooom-m2-settings-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn settings(&self) -> SettingsFile {
            SettingsFile::new(self.0.join("settings.json"))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_file(self.0.join("settings.json"));
            let _ = fs::remove_dir(&self.0);
        }
    }

    #[test]
    fn a_missing_file_is_populated_and_valid_settings_reload() {
        let fixture = TestDirectory::new();
        let file = fixture.settings();
        assert_eq!(file.load(), (Settings::default(), None));
        assert!(file.path.is_file());
        let changed = Settings {
            edge: Edge::Bottom,
            capture_exclusion: false,
            ..Settings::default()
        };
        file.save(&changed, true).unwrap();
        assert_eq!(file.load(), (changed, None));
    }

    #[test]
    fn creating_defaults_leaves_a_concurrently_created_file_intact() {
        let fixture = TestDirectory::new();
        let file = fixture.settings();
        let saved = Settings {
            edge: Edge::Left,
            ..Settings::default()
        };
        file.save(&saved, false).unwrap();
        assert_eq!(
            file.save(&Settings::default(), false).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(file.load(), (saved, None));
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    }

    #[test]
    fn invalid_file_reports_an_issue_and_preserves_its_bytes() {
        let fixture = TestDirectory::new();
        let file = fixture.settings();
        let original = b"{invalid";
        fs::write(&file.path, original).unwrap();
        assert_eq!(
            file.load(),
            (Settings::default(), Some(SettingsIssue::Invalid))
        );
        assert_eq!(fs::read(&file.path).unwrap(), original);
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_sharing_violation_preserves_existing_settings() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

        let fixture = TestDirectory::new();
        let file = fixture.settings();
        file.save(&Settings::default(), false).unwrap();
        let original = fs::read(&file.path).unwrap();
        let reader = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&file.path)
            .unwrap();
        let changed = Settings {
            edge: Edge::Left,
            ..Settings::default()
        };
        assert!(file.save(&changed, true).is_err());
        drop(reader);
        assert_eq!(fs::read(&file.path).unwrap(), original);
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    }
}
