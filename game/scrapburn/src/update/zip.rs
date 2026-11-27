//! A small, strict zip reader for unpacking release archives.
//!
//! It reads exactly what `tools/package.py` writes (stored or deflated entries, no
//! zip64, no encryption) and refuses anything else rather than guessing. Every entry
//! name is checked so nothing can be written outside the destination, symlinks are
//! rejected, and each entry's size and CRC-32 must match the central directory.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use flate2::Crc;
use flate2::read::DeflateDecoder;

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const EOCD_LEN: usize = 22;
/// The end-of-central-directory record plus the longest possible comment.
const EOCD_SEARCH: u64 = (EOCD_LEN + u16::MAX as usize) as u64;
const HOST_UNIX: u16 = 3;
const S_IFMT: u32 = 0o170_000;
const S_IFLNK: u32 = 0o120_000;

#[derive(Debug, thiserror::Error)]
pub enum ZipError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("not a zip archive")]
    NotZip,
    #[error("unsupported zip archive: {0}")]
    Unsupported(&'static str),
    #[error("corrupt zip archive: {0}")]
    Corrupt(&'static str),
    #[error("unsafe path in archive: {0}")]
    UnsafePath(String),
    #[error("archive unpacks to more than the size limit")]
    TooLarge,
}

#[derive(Debug)]
struct Entry {
    name: String,
    method: u16,
    crc: u32,
    compressed: u64,
    size: u64,
    local_offset: u64,
    /// Unix permission bits, when the archive was made on a Unix host.
    mode: Option<u32>,
}

/// Unpacks `archive` into `dest`, which must not exist yet. Stops once the entries
/// would unpack to more than `limit` bytes.
pub fn extract(archive: &Path, dest: &Path, limit: u64) -> Result<(), ZipError> {
    let mut file = File::open(archive)?;
    let entries = central_directory(&mut file)?;
    fs::create_dir(dest)?;
    let mut budget = limit;
    for entry in &entries {
        let relative = safe_path(&entry.name)?;
        let target = dest.join(&relative);
        if entry.name.ends_with('/') {
            fs::create_dir_all(&target)?;
            continue;
        }
        if entry.mode.is_some_and(|m| m & S_IFMT == S_IFLNK) {
            return Err(ZipError::UnsafePath(format!("{} is a symlink", entry.name)));
        }
        budget = budget.checked_sub(entry.size).ok_or(ZipError::TooLarge)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        write_entry(&mut file, entry, &target)?;
    }
    Ok(())
}

fn write_entry(file: &mut File, entry: &Entry, target: &Path) -> Result<(), ZipError> {
    file.seek(SeekFrom::Start(entry.local_offset))?;
    let mut header = [0u8; 30];
    file.read_exact(&mut header)?;
    if u32_at(&header, 0) != LOCAL_SIGNATURE {
        return Err(ZipError::Corrupt("bad local header"));
    }
    let skip = u64::from(u16_at(&header, 26)) + u64::from(u16_at(&header, 28));
    file.seek(SeekFrom::Current(skip as i64))?;
    let raw = (&mut *file).take(entry.compressed);
    let mut data: Box<dyn Read> = match entry.method {
        0 => Box::new(raw),
        8 => Box::new(DeflateDecoder::new(raw)),
        _ => return Err(ZipError::Unsupported("compression method")),
    };
    // create_new: a name that appears twice is an error, not a silent overwrite.
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let mut crc = Crc::new();
    let mut written = 0u64;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = data.read(&mut buf)?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > entry.size {
            return Err(ZipError::Corrupt("entry larger than recorded"));
        }
        crc.update(&buf[..n]);
        out.write_all(&buf[..n])?;
    }
    if written != entry.size || crc.sum() != entry.crc {
        return Err(ZipError::Corrupt("entry size or checksum mismatch"));
    }
    out.flush()?;
    set_mode(&out, entry.mode)?;
    Ok(())
}

#[cfg(unix)]
fn set_mode(file: &File, mode: Option<u32>) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match mode {
        Some(m) => file.set_permissions(fs::Permissions::from_mode(m & 0o755)),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
fn set_mode(_: &File, _: Option<u32>) -> io::Result<()> {
    Ok(())
}

fn central_directory(file: &mut File) -> Result<Vec<Entry>, ZipError> {
    let len = file.seek(SeekFrom::End(0))?;
    let tail_len = len.min(EOCD_SEARCH);
    file.seek(SeekFrom::Start(len - tail_len))?;
    let mut tail = vec![0u8; tail_len as usize];
    file.read_exact(&mut tail)?;
    let eocd = (0..=tail.len().saturating_sub(EOCD_LEN))
        .rev()
        .find(|&i| u32_at(&tail, i) == EOCD_SIGNATURE)
        .ok_or(ZipError::NotZip)?;
    let record = &tail[eocd..];
    if u16_at(record, 4) != 0 || u16_at(record, 6) != 0 {
        return Err(ZipError::Unsupported("multi-disk archive"));
    }
    let count = u16_at(record, 10);
    let size = u32_at(record, 12);
    let offset = u32_at(record, 16);
    if count == u16::MAX || size == u32::MAX || offset == u32::MAX {
        return Err(ZipError::Unsupported("zip64"));
    }
    if u64::from(offset) + u64::from(size) > len {
        return Err(ZipError::Corrupt("central directory out of range"));
    }
    file.seek(SeekFrom::Start(offset.into()))?;
    let mut directory = vec![0u8; size as usize];
    file.read_exact(&mut directory)?;
    let mut entries = Vec::with_capacity(count.into());
    let mut at = 0usize;
    for _ in 0..count {
        let header = directory
            .get(at..at + 46)
            .ok_or(ZipError::Corrupt("truncated central directory"))?;
        if u32_at(header, 0) != CENTRAL_SIGNATURE {
            return Err(ZipError::Corrupt("bad central directory entry"));
        }
        let made_by = u16_at(header, 4);
        let flags = u16_at(header, 8);
        if flags & 1 != 0 {
            return Err(ZipError::Unsupported("encrypted entry"));
        }
        let name_len = usize::from(u16_at(header, 28));
        let extra_len = usize::from(u16_at(header, 30));
        let comment_len = usize::from(u16_at(header, 32));
        let external = u32_at(header, 38);
        let name = directory
            .get(at + 46..at + 46 + name_len)
            .ok_or(ZipError::Corrupt("truncated entry name"))?;
        let name = std::str::from_utf8(name)
            .map_err(|_| ZipError::UnsafePath("non-UTF-8 name".into()))?
            .to_string();
        let compressed = u32_at(header, 20);
        let size = u32_at(header, 24);
        let local_offset = u32_at(header, 42);
        if compressed == u32::MAX || size == u32::MAX || local_offset == u32::MAX {
            return Err(ZipError::Unsupported("zip64"));
        }
        entries.push(Entry {
            name,
            method: u16_at(header, 10),
            crc: u32_at(header, 16),
            compressed: compressed.into(),
            size: size.into(),
            local_offset: local_offset.into(),
            mode: (made_by >> 8 == HOST_UNIX).then_some(external >> 16),
        });
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(entries)
}

/// The entry name as a relative path, or an error if it could land outside the
/// destination or mean something special on Windows.
fn safe_path(name: &str) -> Result<PathBuf, ZipError> {
    let trimmed = name.strip_suffix('/').unwrap_or(name);
    let unsafe_path = || ZipError::UnsafePath(name.to_string());
    if trimmed.is_empty() || name.starts_with('/') {
        return Err(unsafe_path());
    }
    let mut path = PathBuf::new();
    for part in trimmed.split('/') {
        let bad = part.is_empty()
            || part == "."
            || part == ".."
            || part.contains(['\\', ':', '\0'])
            || part.ends_with(['.', ' ']);
        if bad {
            return Err(unsafe_path());
        }
        path.push(part);
    }
    Ok(path)
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::DeflateEncoder;

    /// One archive member for [`build`]: name, contents, Unix mode, deflate it.
    pub(crate) struct Member<'a>(pub &'a str, pub &'a [u8], pub u32, pub bool);

    /// Writes a minimal zip archive the way Python's zipfile lays it out.
    pub(crate) fn build(members: &[Member]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for Member(name, data, mode, deflate) in members {
            let mut crc = Crc::new();
            crc.update(data);
            let body = if *deflate {
                let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
                e.write_all(data).unwrap();
                e.finish().unwrap()
            } else {
                data.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let offset = out.len() as u32;
            let common = |v: &mut Vec<u8>| {
                v.extend(20u16.to_le_bytes());
                v.extend(0u16.to_le_bytes());
                v.extend(method.to_le_bytes());
                v.extend([0u8; 4]);
                v.extend(crc.sum().to_le_bytes());
                v.extend((body.len() as u32).to_le_bytes());
                v.extend((data.len() as u32).to_le_bytes());
                v.extend((name.len() as u16).to_le_bytes());
                v.extend(0u16.to_le_bytes());
            };
            out.extend(LOCAL_SIGNATURE.to_le_bytes());
            common(&mut out);
            out.extend(name.as_bytes());
            out.extend(&body);
            central.extend(CENTRAL_SIGNATURE.to_le_bytes());
            central.extend(((HOST_UNIX << 8) | 20).to_le_bytes());
            common(&mut central);
            // Comment length, disk number, internal attributes.
            central.extend([0u8; 6]);
            central.extend((mode << 16).to_le_bytes());
            central.extend(offset.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend(&central);
        out.extend(EOCD_SIGNATURE.to_le_bytes());
        out.extend([0u8; 4]);
        out.extend((members.len() as u16).to_le_bytes());
        out.extend((members.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(cd_offset.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    pub(crate) fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("scrapburn-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn unpack(tag: &str, members: &[Member]) -> (PathBuf, Result<(), ZipError>) {
        let dir = scratch(tag);
        let archive = dir.join("a.zip");
        fs::write(&archive, build(members)).unwrap();
        let out = dir.join("out");
        let result = extract(&archive, &out, 1 << 20);
        (out, result)
    }

    #[test]
    fn extracts_stored_and_deflated_entries_with_modes() {
        let (out, result) = unpack(
            "zip-ok",
            &[
                Member("game/", b"", 0o040_755, false),
                Member("game/scrapburn", b"#!binary", 0o100_755, true),
                Member("game/assets/a.txt", b"hello hello hello", 0o100_644, false),
            ],
        );
        result.unwrap();
        assert_eq!(fs::read(out.join("game/scrapburn")).unwrap(), b"#!binary");
        assert_eq!(
            fs::read(out.join("game/assets/a.txt")).unwrap(),
            b"hello hello hello"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(out.join("game/scrapburn"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "executable bit survives");
        }
    }

    #[test]
    fn rejects_paths_that_escape_the_destination() {
        for name in [
            "../evil",
            "/abs",
            "a/../../b",
            "c:\\x",
            "a\\..\\b",
            "con.",
            "a//b",
        ] {
            let (_, result) = unpack("zip-escape", &[Member(name, b"x", 0o100_644, false)]);
            assert!(
                matches!(result, Err(ZipError::UnsafePath(_))),
                "{name}: {result:?}"
            );
        }
    }

    #[test]
    fn rejects_symlinks_duplicates_and_oversize() {
        let (_, link) = unpack("zip-link", &[Member("l", b"/etc/passwd", 0o120_777, false)]);
        assert!(matches!(link, Err(ZipError::UnsafePath(_))));
        let (_, dup) = unpack(
            "zip-dup",
            &[
                Member("a", b"1", 0o100_644, false),
                Member("a", b"2", 0o100_644, false),
            ],
        );
        assert!(matches!(dup, Err(ZipError::Io(_))));
        let dir = scratch("zip-big");
        let archive = dir.join("a.zip");
        fs::write(
            &archive,
            build(&[Member("a", &[7u8; 100], 0o100_644, true)]),
        )
        .unwrap();
        assert!(matches!(
            extract(&archive, &dir.join("out"), 99),
            Err(ZipError::TooLarge)
        ));
    }

    #[test]
    fn rejects_corruption() {
        let mut bytes = build(&[Member("a", b"payload!", 0o100_644, false)]);
        let at = bytes.windows(8).position(|w| w == b"payload!").unwrap();
        bytes[at] ^= 0xff;
        let dir = scratch("zip-crc");
        let archive = dir.join("a.zip");
        fs::write(&archive, &bytes).unwrap();
        assert!(matches!(
            extract(&archive, &dir.join("out"), 1 << 20),
            Err(ZipError::Corrupt(_))
        ));
        fs::write(&archive, b"not a zip at all").unwrap();
        assert!(matches!(
            extract(&archive, &dir.join("out2"), 1 << 20),
            Err(ZipError::NotZip)
        ));
    }
}
