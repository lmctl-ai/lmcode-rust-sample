//! Raw-byte file helpers. Paths and file contents never undergo UTF-8 decoding on Unix.
use crate::{ByteString, Context, Error, Registry, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

fn failure(function: &str, path: &ByteString, error: impl std::fmt::Display) -> Error {
    let display = match std::str::from_utf8(path.as_bytes()) {
        Ok(text) => text.to_string(),
        Err(_) => format!("bytes:{}", crate::hex(path.as_bytes())),
    };
    Error::host(format!("{}: {}: {}", function, display, error))
}
fn reject_nul(function: &str, path: &ByteString) -> Result<(), Error> {
    if path.as_bytes().contains(&0) { Err(failure(function, path, "invalid argument")) } else { Ok(()) }
}
#[cfg(unix)]
fn native_path(function: &str, path: &ByteString) -> Result<PathBuf, Error> {
    use std::os::unix::ffi::OsStringExt;
    reject_nul(function, path)?;
    Ok(PathBuf::from(std::ffi::OsString::from_vec(path.as_bytes().to_vec())))
}
#[cfg(windows)]
fn native_path(function: &str, path: &ByteString) -> Result<PathBuf, Error> {
    reject_nul(function, path)?;
    let text = std::str::from_utf8(path.as_bytes()).map_err(|e| failure(function, path, e))?;
    Ok(PathBuf::from(text))
}

fn regular(function: &str, path: &ByteString, allow_missing: bool) -> Result<Option<fs::Metadata>, Error> {
    let native = native_path(function, path)?;
    match fs::metadata(native) {
        Ok(metadata) if metadata.is_file() => Ok(Some(metadata)),
        Ok(metadata) => Err(failure(function, path, if metadata.is_dir() { "is a directory" } else { "invalid argument" })),
        Err(error) if allow_missing && matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => Ok(None),
        Err(error) => Err(failure(function, path, error)),
    }
}
fn exists(function: &str, path: &ByteString) -> Result<bool, Error> {
    if path.as_bytes().is_empty() { return Ok(false); }
    let native = native_path(function, path)?;
    match fs::metadata(native) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => Ok(false),
        Err(error) => Err(failure(function, path, error)),
    }
}
fn read_bytes(function: &str, path: &ByteString) -> Result<Vec<u8>, Error> {
    regular(function, path, false)?;
    let native = native_path(function, path)?;
    let mut file = File::open(native).map_err(|e| failure(function, path, e))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| failure(function, path, e))?;
    close_checked(file).map_err(|e| failure(function, path, e))?;
    Ok(bytes)
}
fn write_bytes(function: &str, path: &ByteString, bytes: &[u8], append: bool) -> Result<(), Error> {
    regular(function, path, true)?;
    let native = native_path(function, path)?;
    let mut options = OpenOptions::new(); options.create(true).write(true);
    if append { options.append(true); } else { options.truncate(true); }
    let mut file = options.open(native).map_err(|e| failure(function, path, e))?;
    file.write_all(bytes).map_err(|e| failure(function, path, e))?;
    file.flush().map_err(|e| failure(function, path, e))?;
    file.sync_all().map_err(|e| failure(function, path, e))?;
    close_checked(file).map_err(|e| failure(function, path, e))
}
#[cfg(unix)]
fn close_checked(file: File) -> std::io::Result<()> {
    use std::os::fd::IntoRawFd;
    let fd = file.into_raw_fd();
    unsafe { if close(fd) == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) } }
}
#[cfg(unix)]
unsafe extern "C" { fn close(fd: std::ffi::c_int) -> std::ffi::c_int; }
#[cfg(windows)]
fn close_checked(file: File) -> std::io::Result<()> {
    use std::os::windows::io::IntoRawHandle;
    let handle = file.into_raw_handle();
    unsafe { if CloseHandle(handle) != 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) } }
}
#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" { fn CloseHandle(handle: *mut std::ffi::c_void) -> i32; }
fn lines(bytes: &[u8]) -> Vec<ByteString> {
    let mut output = Vec::new(); let mut start = 0;
    for (i, byte) in bytes.iter().enumerate() { if *byte == b'\n' { output.push(ByteString::from(&bytes[start..i])); start = i + 1; } }
    output.push(ByteString::from(&bytes[start..])); output
}

/// Register raw-byte regular-file helpers and `file_lines` using the context-owned StrList arena.
pub fn register_file(registry: &mut Registry) -> Result<&mut Registry, Error> {
    registry.begin_compatibility_registration();
    registry.function("file_read", &["string"], "string", |_, args| Ok(Value::string(read_bytes("file_read", args[0].as_string()?)?)))?;
    registry.function("file_write", &["string", "string"], "void", |_, args| { let path = args[0].as_string()?; write_bytes("file_write", path, args[1].as_bytes()?, false)?; Ok(Value::None) })?;
    registry.function("file_append", &["string", "string"], "void", |_, args| { let path = args[0].as_string()?; write_bytes("file_append", path, args[1].as_bytes()?, true)?; Ok(Value::None) })?;
    registry.function("file_exists", &["string"], "bool", |_, args| Ok(Value::Bool(exists("file_exists", args[0].as_string()?)?)))?;
    registry.function("file_lines", &["string"], "StrList", |ctx: &mut Context, args| {
        let bytes = read_bytes("file_lines", args[0].as_string()?)?;
        Ok(Value::object("StrList", ctx.insert_collection("StrList", crate::stdlib::collections::CollectionList { items: lines(&bytes) })?))
    })?;
    registry.end_compatibility_registration(); Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_file_round_trip_and_lf_lines() {
        let root = std::env::temp_dir().join(format!("lmcode-rust-file-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("raw.bin");
        let path = ByteString::from(path.to_string_lossy().as_bytes());
        write_bytes("file_write", &path, b"a\n\xff\n", false).unwrap();
        assert_eq!(read_bytes("file_read", &path).unwrap(), b"a\n\xff\n");
        assert_eq!(lines(&read_bytes("file_read", &path).unwrap()), vec![ByteString::from(b"a"), ByteString::from(b"\xff"), ByteString::from(b"")]);
        write_bytes("file_append", &path, b"z", true).unwrap();
        assert_eq!(read_bytes("file_read", &path).unwrap(), b"a\n\xff\nz");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn exists_distinguishes_missing_and_invalid_paths() {
        let missing = ByteString::from(std::env::temp_dir().join(format!("lmcode-rust-file-missing-{}", std::process::id())).to_string_lossy().as_bytes());
        assert!(regular("file_exists", &missing, true).unwrap().is_none());
        let nul = ByteString::from(b"bad\0path".as_slice());
        assert_eq!(regular("file_exists", &nul, true).unwrap_err().code, "RUN_HOST");
        assert!(!exists("file_exists", &ByteString::from(b"".as_slice())).unwrap());
        #[cfg(unix)] {
            let special = ByteString::from(b"/dev/null".as_slice());
            assert!(!exists("file_exists", &special).unwrap());
            assert!(read_bytes("file_read", &special).is_err());
            assert!(write_bytes("file_write", &special, b"x", false).is_err());
            use std::os::unix::ffi::{OsStrExt, OsStringExt};
            let root = std::env::temp_dir().join(format!("lmcode-rust-raw-{}", std::process::id())); std::fs::create_dir_all(&root).unwrap();
            let raw = root.join(std::ffi::OsString::from_vec(vec![b'x', 0xff]));
            let raw_bytes = ByteString::from(raw.as_os_str().as_bytes());
            match write_bytes("file_write", &raw_bytes, b"raw", false) {
                Ok(()) => assert_eq!(read_bytes("file_read", &raw_bytes).unwrap(), b"raw"),
                Err(error) => {
                    #[cfg(target_os = "macos")] assert!(error.message.contains("Illegal byte sequence"), "{}", error.message);
                    #[cfg(not(target_os = "macos"))] panic!("{}", error.message);
                }
            }
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
