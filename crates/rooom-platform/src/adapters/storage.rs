use crate::pipe::Scope;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::windows::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
    ptr::null_mut,
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER,
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, GetAce, INHERIT_ONLY_ACE,
        OBJECT_INHERIT_ACE, OWNER_SECURITY_INFORMATION, SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::{
        CreateDirectoryW, FILE_ATTRIBUTE_REPARSE_POINT, MOVEFILE_REPLACE_EXISTING,
        MOVEFILE_WRITE_THROUGH, MoveFileExW,
    },
};

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

pub fn create_private(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let scope = Scope::current()?;
    let sddl: Vec<_> = format!("O:{0}D:P(A;OICI;FA;;;{0})", scope.sid())
        .encode_utf16()
        .chain([0])
        .collect();
    let mut descriptor = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let result = if unsafe { CreateDirectoryW(wide(path).as_ptr(), &attributes) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    };
    unsafe { LocalFree(descriptor) };
    if let Err(error) = result
        && error.kind() != io::ErrorKind::AlreadyExists
    {
        return Err(error);
    }
    verify_private(path)
}

pub fn verify_private(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !path.is_absolute()
        || !metadata.is_dir()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let scope = Scope::current()?;
    let (mut owner, mut acl, mut descriptor) = (null_mut(), null_mut(), null_mut());
    let result = unsafe {
        GetNamedSecurityInfoW(
            wide(path).as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut descriptor,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let result = (|| {
        if owner.is_null() || acl.is_null() || sid_text(owner)? != scope.sid() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let mut inherited_access = false;
        for i in 0..u32::from(unsafe { (*acl).AceCount }) {
            let mut raw = null_mut();
            if unsafe { GetAce(acl, i, &mut raw) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let header = unsafe { &*raw.cast::<ACE_HEADER>() };
            if header.AceType != 0 || usize::from(header.AceSize) < size_of::<ACCESS_ALLOWED_ACE>()
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            let ace = raw.cast::<ACCESS_ALLOWED_ACE>();
            if sid_text(unsafe { std::ptr::addr_of!((*ace).SidStart).cast_mut().cast() })?
                != scope.sid()
            {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            let flags = u32::from(header.AceFlags);
            inherited_access |= flags & (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE)
                == (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE)
                && flags & INHERIT_ONLY_ACE == 0;
        }
        inherited_access
            .then_some(())
            .ok_or(io::ErrorKind::PermissionDenied.into())
    })();
    unsafe { LocalFree(descriptor) };
    result
}

fn sid_text(sid: windows_sys::Win32::Security::PSID) -> io::Result<String> {
    let mut output = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut output) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let len = (0..)
        .take_while(|i| unsafe { *output.add(*i) != 0 })
        .count();
    let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(output, len) });
    unsafe { LocalFree(output.cast()) };
    Ok(text)
}

/// Same-directory publication with optional replacement and flushed bytes.
pub fn publish(path: &Path, bytes: &[u8], replace: bool) -> io::Result<()> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let pending = path.with_file_name(format!(".rooom-{}-{sequence}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)?;
    let written = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = written.and_then(|_| move_file(&pending, path, replace));
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

pub fn move_file(from: &Path, to: &Path, replace: bool) -> io::Result<()> {
    let flags = MOVEFILE_WRITE_THROUGH
        | if replace {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    if unsafe { MoveFileExW(wide(from).as_ptr(), wide(to).as_ptr(), flags) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
