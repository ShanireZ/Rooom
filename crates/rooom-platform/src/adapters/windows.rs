mod desktop;
pub use desktop::DesktopWindow;

use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::Path,
    ptr::null_mut,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, GetFileVersionInfoSizeW,
    GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;

pub(super) fn workspace_label(cwd: Option<&str>) -> Option<String> {
    let path = Path::new(cwd?);
    if !path.is_absolute() {
        return None;
    }
    let name = path.file_name()?.to_str()?;
    displayable_folder_name(name).then(|| name.to_owned())
}

fn displayable_folder_name(name: &str) -> bool {
    !name.trim().is_empty()
        && !matches!(name, "." | "..")
        && name.encode_utf16().count() <= 255
        && !name.chars().any(|ch| {
            ch.is_control() || matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
}

pub(super) fn file_uri_workspace_label(uri: Option<&str>) -> Option<String> {
    let encoded = uri?.strip_prefix("file:///")?;
    if encoded.len() > 32 * 1024 || encoded.contains(['\\', '?', '#']) {
        return None;
    }
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut bytes = encoded.bytes();
    while let Some(byte) = bytes.next() {
        if byte != b'%' {
            decoded.push(byte);
            continue;
        }
        let high = char::from(bytes.next()?).to_digit(16)?;
        let low = char::from(bytes.next()?).to_digit(16)?;
        let byte = (high * 16 + low) as u8;
        if matches!(byte, b'/' | b'\\') {
            return None;
        }
        decoded.push(byte);
    }
    let decoded = String::from_utf8(decoded).ok()?;
    let (drive, folders) = decoded.split_once('/')?;
    if !matches!(drive.as_bytes(), [letter, b':'] if letter.is_ascii_alphabetic()) {
        return None;
    }
    let mut name = None;
    for folder in folders.trim_end_matches('/').split('/') {
        if !displayable_folder_name(folder) || folder.contains(['<', '>', ':', '"', '|', '?', '*'])
        {
            return None;
        }
        name = Some(folder);
    }
    name.map(str::to_owned)
}

#[cfg(test)]
#[test]
fn workspace_label_only_shows_a_safe_basename_from_an_absolute_path() {
    assert_eq!(
        workspace_label(Some(r"C:\fixture\專案 Workspace\")).as_deref(),
        Some("專案 Workspace")
    );
    for invalid in [
        None,
        Some(r"relative\project"),
        Some(r"C:\"),
        Some("C:\\fixture\\with\nline"),
        Some("C:\\fixture\\with\u{202e}override"),
    ] {
        assert_eq!(workspace_label(invalid), None);
    }
}

#[cfg(test)]
#[test]
fn file_uri_workspace_decodes_the_local_folder_once() {
    assert_eq!(
        file_uri_workspace_label(Some("file:///C%3A/test/%E6%B8%AC%E8%A9%A6%20%25notes/"))
            .as_deref(),
        Some("測試 %notes")
    );
    for invalid in [
        "https://example.org/Test",
        "file://remote/C:/Test",
        "file:///C:/",
        "file:///C:/Test%2FOther",
        "file:///C:/%2E%2E/Test",
        "file:///C:/test/%FF",
        "file:///C:/test/%",
        "file:///C:/test/%00Test",
        "file:///C:/test/Name#section",
    ] {
        assert_eq!(file_uri_workspace_label(Some(invalid)), None);
    }
}

pub(super) fn session_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

pub(super) fn execute_uri(uri: &str, show: i32) -> bool {
    let uri: Vec<_> = uri.encode_utf16().chain([0]).collect();
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            std::ptr::null(),
            uri.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            show,
        )
    };
    result as isize > 32
}

pub(super) fn open_plain_file(path: &Path) -> io::Result<(File, Metadata)> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok((file, metadata))
}

pub(super) fn file_stamp(metadata: &Metadata) -> (u64, u64, u64) {
    (
        metadata.len(),
        metadata.creation_time(),
        metadata.last_write_time(),
    )
}

pub(super) fn jsonl_tail(
    file: &mut File,
    stamp: (u64, u64, u64),
    records_start: u64,
    window: usize,
) -> io::Result<(u64, Vec<u8>)> {
    let len = stamp.0;
    if records_start > len {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let start = records_start.max(len.saturating_sub(window as u64));
    // The preceding byte distinguishes a cut line from a record boundary.
    let read_start = start - u64::from(start > records_start);
    let size = len - read_start;
    file.seek(SeekFrom::Start(read_start))?;
    let mut bytes = Vec::with_capacity(size as usize);
    (&mut *file).take(size).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size || file_stamp(&file.metadata()?) != stamp {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let first = if read_start < start {
        bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |index| index + 1)
    } else {
        0
    };
    bytes.drain(..first);
    Ok((read_start + first as u64, bytes))
}

pub fn product_version(path: &Path) -> io::Result<[u16; 4]> {
    if !path.is_absolute() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let path: Vec<_> = fs::canonicalize(path)?
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect();
    let size = unsafe { GetFileVersionInfoSizeW(path.as_ptr(), null_mut()) };
    if size == 0 || size > 1024 * 1024 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut data = vec![0u8; size as usize];
    if unsafe { GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let (mut info, mut length) = (null_mut(), 0);
    if unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            [92, 0].as_ptr(),
            &mut info,
            &mut length,
        )
    } == 0
        || info.is_null()
        || (length as usize) < size_of::<VS_FIXEDFILEINFO>()
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    // The version resource can return an unaligned pointer into the buffer.
    let info = unsafe { info.cast::<VS_FIXEDFILEINFO>().read_unaligned() };
    if info.dwSignature != 0xfeef04bd {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok([
        (info.dwProductVersionMS >> 16) as u16,
        info.dwProductVersionMS as u16,
        (info.dwProductVersionLS >> 16) as u16,
        info.dwProductVersionLS as u16,
    ])
}
