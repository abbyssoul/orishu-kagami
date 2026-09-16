//! Shared strict stored-ZIP framing for plugin, experiment and workload containers.
//!
//! Only one explicit root and `blobs/sha256/<digest>` regular entries are admitted.
//! The intentionally minimal v1 profile has no extra fields, comments, data
//! descriptors, encryption, compression, ZIP64 or spanning. Local/central sizes,
//! names, CRCs and flags must agree. Central order and timestamps are not identity.
//! See PKWARE APPNOTE 6.3.10 sections 4.3.7, 4.3.12 and 4.3.16 for record framing.
//! Reading checks framing/CRCs, not root semantics or declared SHA-256 identity.
//! Callers verify their exact closure and hashes before adopting any blob.

use crate::{ArtifactDigest, Error, ErrorCode};
use std::collections::BTreeMap;

/// Archive budgets in addition to declaration and artifact limits.
#[derive(Clone, Copy, Debug)]
pub struct ArchiveLimits {
    /// Entire archive bytes, including framing and manifest, before parsing.
    pub max_bytes: usize,
    /// Number of physical entries, including the root.
    pub max_entries: usize,
    /// Root metadata bytes before parsing it.
    pub root_bytes: usize,
    /// One digest-addressed blob.
    pub blob_bytes: u64,
    /// Total physical blob bytes before hashing/scanning.
    pub total_blob_bytes: u64,
}
impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_bytes: 1024 * 1024 * 1024,
            max_entries: 4097,
            root_bytes: 1024 * 1024,
            blob_bytes: 256 * 1024 * 1024,
            total_blob_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// Closed set of roots accepted by this profile; callers cannot supply paths.
#[derive(Clone, Copy, Debug)]
pub enum Root {
    /// A plugin bundle's canonical release root.
    Plugin,
    /// A versioned experiment container's authored document.
    Document,
    /// A portable workload's canonical root; distinct from plugin declarations.
    Workload,
}
impl Root {
    fn name(self) -> &'static str {
        match self {
            Self::Plugin => "manifest.cbor",
            Self::Document => "document.json",
            Self::Workload => "workload.cbor",
        }
    }
}

/// Borrowed, CRC-checked entries. No extraction, parsing or code execution.
#[derive(Debug)]
pub struct Archive<'a> {
    /// Opaque root metadata, subject to the caller's versioned decoder.
    pub root: &'a [u8],
    /// Caller-owned bytes keyed by filename digest, still requiring hash checks.
    pub blobs: BTreeMap<ArtifactDigest, &'a [u8]>,
}

fn bad(message: &str) -> Error {
    Error::new(ErrorCode::Malformed, "bundle", message)
}
fn unsupported(message: &str) -> Error {
    Error::new(ErrorCode::UnsupportedVersion, "archive", message)
}
fn limit() -> Error {
    Error::new(
        ErrorCode::LimitExceeded,
        "bundle",
        "archive budget or classic ZIP size exceeded",
    )
}
fn part(b: &[u8], at: usize, len: usize) -> Result<&[u8], Error> {
    b.get(at..at.checked_add(len).ok_or_else(limit)?)
        .ok_or_else(|| bad("truncated archive"))
}
fn u16_at(b: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        part(b, at, 2)?.try_into().expect("checked length"),
    ))
}
fn u32_at(b: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        part(b, at, 4)?.try_into().expect("checked length"),
    ))
}
fn classic(n: u32) -> Result<usize, Error> {
    if n == u32::MAX {
        return Err(unsupported("ZIP64 is not supported"));
    }
    usize::try_from(n).map_err(|_| limit())
}

const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut bit = 0;
        while bit < 8 {
            c = (c >> 1) ^ if c & 1 == 1 { 0xedb8_8320 } else { 0 };
            bit += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}
const CRC_TABLE: [u32; 256] = crc_table();
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &b in bytes {
        crc = CRC_TABLE[((crc ^ u32::from(b)) & 255) as usize] ^ (crc >> 8);
    }
    !crc
}

fn artifact_name(name: &str, root: Root) -> Result<Option<ArtifactDigest>, Error> {
    if name == root.name() {
        return Ok(None);
    }
    let hex = name
        .strip_prefix("blobs/sha256/")
        .filter(|v| v.len() == 64)
        .ok_or_else(|| bad("undeclared or unsafe entry path"))?;
    format!("sha256:{hex}")
        .parse()
        .map(Some)
        .map_err(|_| bad("invalid artifact filename digest"))
}

/// Read and verify archive framing. All ranges/counts are checked before
/// slicing or reserving; artifact buffers are borrowed. Complexity is O(bytes +
/// entries log(entries)). Root interpretation belongs to the caller.
pub fn read<'a>(bytes: &'a [u8], root: Root, archive: ArchiveLimits) -> Result<Archive<'a>, Error> {
    if bytes.len() > archive.max_bytes {
        return Err(limit());
    }
    let end = bytes
        .len()
        .checked_sub(22)
        .ok_or_else(|| bad("missing ZIP footer"))?;
    let footer = part(bytes, end, 22)?;
    if u32_at(footer, 0)? != 0x0605_4b50 || u16_at(footer, 20)? != 0 {
        return Err(bad("expected final comment-free ZIP footer"));
    }
    if u16_at(footer, 4)? != 0 || u16_at(footer, 6)? != 0 {
        return Err(unsupported("multi-disk archive"));
    }
    let count = usize::from(u16_at(footer, 10)?);
    if count == 65535 || count == 0 || count != usize::from(u16_at(footer, 8)?) {
        return Err(bad("invalid entry count or ZIP64"));
    }
    if count > archive.max_entries {
        return Err(limit());
    }
    let central_len = classic(u32_at(footer, 12)?)?;
    let central_start = classic(u32_at(footer, 16)?)?;
    if central_start.checked_add(central_len) != Some(end) {
        return Err(bad("central directory range mismatch"));
    }
    let central = part(bytes, central_start, central_len)?;
    let mut at = 0usize;
    let mut entries = BTreeMap::new();
    let mut ranges = Vec::new();
    let mut artifact_bytes = 0u64;
    for _ in 0..count {
        let h = part(central, at, 46)?;
        if u32_at(h, 0)? != 0x0201_4b50 {
            return Err(bad("invalid central header"));
        }
        let version = u16_at(h, 6)?;
        let flags = u16_at(h, 8)?;
        if !matches!(version, 10 | 20) || flags & !0x0800 != 0 || u16_at(h, 10)? != 0 {
            return Err(unsupported("unsupported archive feature"));
        }
        if u16_at(h, 30)? != 0 || u16_at(h, 32)? != 0 || u16_at(h, 34)? != 0 {
            return Err(unsupported(
                "extra fields, comments and spanning are not in this profile",
            ));
        }
        let attrs = u32_at(h, 38)?;
        let file_type = (attrs >> 16) & 0xf000;
        if attrs & 0x10 != 0 || !matches!(file_type, 0 | 0x8000) {
            return Err(bad("only regular file entries are allowed"));
        }
        let size = classic(u32_at(h, 24)?)?;
        if size != classic(u32_at(h, 20)?)? {
            return Err(bad("stored entry sizes disagree"));
        }
        let name_len = usize::from(u16_at(h, 28)?);
        if name_len > 77 {
            return Err(bad("invalid entry name length"));
        }
        let name_bytes = part(central, at + 46, name_len)?;
        let name = std::str::from_utf8(name_bytes).map_err(|_| bad("non-UTF8 name"))?;
        let digest = artifact_name(name, root)?;
        let max = if digest.is_some() {
            archive.blob_bytes
        } else {
            archive.root_bytes as u64
        };
        if size as u64 > max {
            return Err(limit());
        }
        if digest.is_some() {
            artifact_bytes = artifact_bytes
                .checked_add(size as u64)
                .filter(|n| *n <= archive.total_blob_bytes)
                .ok_or_else(limit)?;
        }
        let local_start = classic(u32_at(h, 42)?)?;
        let local = part(bytes, local_start, 30)?;
        if u32_at(local, 0)? != 0x0403_4b50
            || u16_at(local, 4)? != version
            || u16_at(local, 6)? != flags
            || u16_at(local, 8)? != 0
            || part(local, 10, 16)? != part(h, 12, 16)?
            || u16_at(local, 26)? as usize != name_len
            || u16_at(local, 28)? != 0
        {
            return Err(bad("local and central metadata disagree"));
        }
        let data_start = local_start
            .checked_add(30)
            .and_then(|v| v.checked_add(name_len))
            .ok_or_else(limit)?;
        let data_end = data_start
            .checked_add(size)
            .filter(|v| *v <= central_start)
            .ok_or_else(|| bad("entry overlaps central directory"))?;
        if part(bytes, local_start + 30, name_len)? != name_bytes {
            return Err(bad("local and central filenames disagree"));
        }
        let data = part(bytes, data_start, size)?;
        if entries
            .insert(name, (digest, data, u32_at(h, 16)?))
            .is_some()
        {
            return Err(bad("duplicate archive entry"));
        }
        ranges.push((local_start, data_end));
        at = at.checked_add(46 + name_len).ok_or_else(limit)?;
    }
    if at != central.len() {
        return Err(bad("unaccounted central directory data"));
    }
    ranges.sort_unstable();
    let mut next = 0;
    for (start, end) in ranges {
        if start != next {
            return Err(bad(
                "overlapping entries, prefix, or unaccounted local data",
            ));
        }
        next = end;
    }
    if next != central_start {
        return Err(bad("unaccounted data before directory"));
    }
    // Establish non-overlap before scanning content. Otherwise hostile headers
    // could force repeated CRC work over the same large byte range.
    for (_, data, crc) in entries.values() {
        if crc32(data) != *crc {
            return Err(Error::new(
                ErrorCode::IntegrityMismatch,
                "bundle",
                "ZIP CRC mismatch",
            ));
        }
    }
    let (_, manifest, _) = entries.remove(root.name()).ok_or_else(|| {
        bad(match root {
            Root::Plugin => "manifest is absent",
            Root::Document => "document is absent",
            Root::Workload => "workload is absent",
        })
    })?;
    let artifacts: BTreeMap<_, _> = entries
        .into_values()
        .map(|(digest, bytes, _)| (digest.expect("only root lacks a digest"), bytes))
        .collect();
    Ok(Archive {
        root: manifest,
        blobs: artifacts,
    })
}

fn push16(out: &mut Vec<u8>, n: u16) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn push32(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}

/// Pack exactly the supplied blobs into a deterministic minimal stored ZIP.
/// The caller selects closure and interprets root bytes. Checked total output
/// sizing precedes allocation; no archive metadata enters scientific identity.
pub fn pack(
    root: Root,
    manifest: &[u8],
    blobs: &BTreeMap<ArtifactDigest, &[u8]>,
    archive: ArchiveLimits,
) -> Result<Vec<u8>, Error> {
    if manifest.len() > archive.root_bytes || blobs.len().saturating_add(1) > archive.max_entries {
        return Err(limit());
    }
    let mut total_blobs = 0u64;
    for data in blobs.values() {
        if data.len() as u64 > archive.blob_bytes {
            return Err(limit());
        }
        total_blobs = total_blobs
            .checked_add(data.len() as u64)
            .filter(|n| *n <= archive.total_blob_bytes)
            .ok_or_else(limit)?;
    }
    let mut files = vec![(root.name().to_owned(), manifest)];
    for (digest, data) in blobs {
        files.push((
            format!(
                "blobs/sha256/{}",
                digest.to_string().strip_prefix("sha256:").expect("SHA256")
            ),
            *data,
        ));
    }
    if files.len() >= 65535 || files.len() > archive.max_entries {
        return Err(limit());
    }
    let mut total = 22usize;
    for (name, data) in &files {
        total = total
            .checked_add(76 + 2 * name.len())
            .and_then(|v| v.checked_add(data.len()))
            .ok_or_else(limit)?;
        if data.len() >= u32::MAX as usize {
            return Err(limit());
        }
    }
    if total > archive.max_bytes || total >= u32::MAX as usize {
        return Err(limit());
    }
    for (digest, data) in blobs {
        if !digest.matches(data) {
            return Err(Error::new(
                ErrorCode::IntegrityMismatch,
                "archive",
                "blob digest mismatch",
            ));
        }
    }
    let mut out = Vec::with_capacity(total);
    let mut central = Vec::new();
    for (name, data) in &files {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        push32(&mut out, 0x0403_4b50);
        push16(&mut out, 20);
        push16(&mut out, 0x0800);
        push16(&mut out, 0);
        push16(&mut out, 0);
        push16(&mut out, 0x21);
        push32(&mut out, crc);
        push32(&mut out, size);
        push32(&mut out, size);
        push16(&mut out, name.len() as u16);
        push16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.push((name, offset, crc, size));
    }
    let central_start = out.len() as u32;
    for (name, offset, crc, size) in central {
        push32(&mut out, 0x0201_4b50);
        push16(&mut out, 0x0314);
        push16(&mut out, 20);
        push16(&mut out, 0x0800);
        push16(&mut out, 0);
        push16(&mut out, 0);
        push16(&mut out, 0x21);
        push32(&mut out, crc);
        push32(&mut out, size);
        push32(&mut out, size);
        push16(&mut out, name.len() as u16);
        push16(&mut out, 0);
        push16(&mut out, 0);
        push16(&mut out, 0);
        push16(&mut out, 0);
        push32(&mut out, 0o100600 << 16);
        push32(&mut out, offset);
        out.extend_from_slice(name.as_bytes());
    }
    let central_size = out.len() as u32 - central_start;
    push32(&mut out, 0x0605_4b50);
    push16(&mut out, 0);
    push16(&mut out, 0);
    push16(&mut out, files.len() as u16);
    push16(&mut out, files.len() as u16);
    push32(&mut out, central_size);
    push32(&mut out, central_start);
    push16(&mut out, 0);
    debug_assert_eq!(out.len(), total);
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn standard_crc_vector() {
        assert_eq!(super::crc32(b"123456789"), 0xcbf4_3926);
    }
}
