mod support;

use orishu_plugin::{bundle::*, *};
use support::*;

fn fixture() -> Vec<u8> {
    let (root, blobs) = release(
        "org.example.complete",
        &declarations(),
        &[b"classical-kernel-fixture", b"euler-kernel-fixture"],
    );
    pack(
        &root,
        &borrowed(&blobs),
        &Limits::default(),
        BundleLimits::default(),
    )
    .unwrap()
}
fn u16_at(b: &[u8], at: usize) -> usize {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap()) as usize
}
fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}
fn set16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn set32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn directory(b: &[u8]) -> Vec<usize> {
    let footer = b.len() - 22;
    let mut at = u32_at(b, footer + 16);
    (0..u16_at(b, footer + 10))
        .map(|_| {
            let current = at;
            at += 46 + u16_at(b, at + 28);
            current
        })
        .collect()
}
fn read_fixture(b: &[u8]) -> Result<Bundle<'_>, Error> {
    read(b, &Limits::default(), BundleLimits::default())
}
fn crc32(b: &[u8]) -> u32 {
    // Independent bitwise reference (production uses a lookup table).
    let mut crc = !0u32;
    for &v in b {
        crc ^= u32::from(v);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !crc
}

#[test]
fn python_zipfile_interoperability_golden() {
    // Produced independently with Python stdlib zipfile.ZipFile/ZipInfo,
    // ZIP_STORED, timestamp (2026, 9, 16, 12, 30, 0), default permissions;
    // testzip() and read() verified this before it was captured as hex.
    let hex = include_str!("fixtures/python-stored-empty.hex").trim();
    let bytes: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let bundle = read_fixture(&bytes).unwrap();
    assert_eq!(
        bundle.release().id().to_string(),
        "sha256:e20728014c789ccec8a0a6573f1668dea2eda29ad2f45e3f7f99c4a18ee44843"
    );
    let mut packed = pack(
        bundle.release().root(),
        bundle.artifacts(),
        &Limits::default(),
        BundleLimits::default(),
    )
    .unwrap();
    let at = directory(&packed)[0];
    // Normalize only deliberate non-identity ZIP metadata differences. All
    // framing, CRCs, lengths, offsets, payload and footer must match Python.
    set16(&mut packed, 6, 0);
    set16(&mut packed, at + 8, 0);
    set32(&mut packed, 10, 0x5d30_63c0);
    set32(&mut packed, at + 12, 0x5d30_63c0);
    set32(&mut packed, at + 38, 0o600 << 16);
    assert_eq!(packed, bytes);
}

#[test]
fn exact_closure_roundtrip_and_deterministic_packing() {
    let original = fixture();
    let bundle = read_fixture(&original).unwrap();
    assert_eq!(bundle.release().root().0.spec.contributions.len(), 7);
    let mut blobs = bundle.artifacts().clone();
    // A cache may contain unrelated code, but pack never exports it.
    blobs.insert(ArtifactDigest::sha256_of(b"unselected"), b"unselected");
    let packed = pack(
        bundle.release().root(),
        &blobs,
        &Limits::default(),
        BundleLimits::default(),
    )
    .unwrap();
    assert_eq!(packed, original);
    assert_eq!(
        read_fixture(&packed).unwrap().release().id(),
        bundle.release().id()
    );
    for (&digest, bytes) in bundle.artifacts() {
        assert!(digest.matches(bytes));
        assert!(bytes.as_ptr() >= original.as_ptr());
        assert!(
            (bytes.as_ptr() as usize + bytes.len()) <= original.as_ptr() as usize + original.len()
        );
    }
}

#[test]
fn central_order_timestamps_permissions_do_not_change_identity() {
    let mut bytes = fixture();
    let expected = read_fixture(&bytes).unwrap().release().id();
    let offsets = directory(&bytes);
    for &at in &offsets {
        let local = u32_at(&bytes, at + 42);
        set32(&mut bytes, at + 12, 0x1234_5678);
        set32(&mut bytes, local + 10, 0x1234_5678);
        set32(&mut bytes, at + 38, 0o100444 << 16);
    }
    let end = bytes.len() - 22;
    let records: Vec<Vec<u8>> = offsets
        .iter()
        .enumerate()
        .map(|(i, &start)| bytes[start..offsets.get(i + 1).copied().unwrap_or(end)].to_vec())
        .collect();
    let mut at = offsets[0];
    for record in records.iter().rev() {
        bytes[at..at + record.len()].copy_from_slice(record);
        at += record.len();
    }
    assert_eq!(read_fixture(&bytes).unwrap().release().id(), expected);
}

#[test]
fn truncated_and_huge_claims_never_panic_or_succeed() {
    let bytes = fixture();
    for len in 0..bytes.len() {
        assert!(read_fixture(&bytes[..len]).is_err(), "prefix {len}");
    }
    let at = directory(&bytes)[0];
    let footer = bytes.len() - 22;
    for field in [at + 20, at + 24, at + 42, footer + 12, footer + 16] {
        let mut bad = bytes.clone();
        set32(&mut bad, field, u32::MAX);
        assert!(read_fixture(&bad).is_err());
    }
}

#[test]
fn rejects_unsupported_zip_features_and_non_regular_entries() {
    let bytes = fixture();
    let at = directory(&bytes)[0];
    let footer = bytes.len() - 22;
    for (offset, value) in [
        (at + 6, 45), // ZIP64 version
        (at + 8, 1),  // encryption
        (at + 8, 8),  // data descriptor
        (at + 10, 8), // deflate
        (at + 30, 1), // extra field
        (at + 32, 1), // comment
        (at + 34, 1), // disk
        (footer + 4, 1),
        (footer + 6, 1),
        (footer + 20, 1),
        (footer + 8, 0),
        (footer + 10, u16::MAX),
    ] {
        let mut bad = bytes.clone();
        set16(&mut bad, offset, value);
        assert!(read_fixture(&bad).is_err(), "offset {offset} value {value}");
    }
    for attrs in [0o120777 << 16, 0o040755 << 16, 0o010600 << 16, 0x10] {
        let mut bad = bytes.clone();
        set32(&mut bad, at + 38, attrs);
        assert!(read_fixture(&bad).is_err());
    }
}

#[test]
fn local_and_central_headers_must_agree() {
    let bytes = fixture();
    let at = directory(&bytes)[0];
    let local = u32_at(&bytes, at + 42);
    for offset in [0, 4, 6, 8, 10, 14, 18, 22, 26, 28, 30] {
        let mut bad = bytes.clone();
        bad[local + offset] ^= 1;
        assert!(read_fixture(&bad).is_err(), "local offset {offset}");
    }
}

#[test]
fn unsafe_paths_duplicate_entries_overlap_and_hidden_data_are_rejected() {
    let bytes = fixture();
    let offsets = directory(&bytes);
    let at = offsets[0];
    let local = u32_at(&bytes, at + 42);
    for name in [
        "../ifest.cbor",
        "/anifest.cbor",
        "MANIFEST.cbor",
        "xanifest.cbor",
    ] {
        let mut bad = bytes.clone();
        assert_eq!(name.len(), 13);
        bad[at + 46..at + 59].copy_from_slice(name.as_bytes());
        bad[local + 30..local + 43].copy_from_slice(name.as_bytes());
        assert!(read_fixture(&bad).is_err());
    }
    // Duplicate an entire central record; both now reference one local entry.
    let mut duplicate = bytes.clone();
    let from = offsets[1];
    let to = offsets[2];
    let record = duplicate[from..to].to_vec();
    duplicate[to..to + record.len()].copy_from_slice(&record);
    assert!(read_fixture(&duplicate).is_err());
    // Prefixing even harmless bytes is forbidden; rebase all directory pointers.
    let mut prefixed = vec![0];
    prefixed.extend_from_slice(&bytes);
    for &at in &offsets {
        let offset = u32_at(&bytes, at + 42) as u32;
        set32(&mut prefixed, at + 1 + 42, offset + 1);
    }
    let footer = prefixed.len() - 22;
    set32(&mut prefixed, footer + 16, offsets[0] as u32 + 1);
    assert!(read_fixture(&prefixed).is_err());
}

#[test]
fn crc_is_not_a_substitute_for_digest_verification() {
    let mut bytes = fixture();
    let offsets = directory(&bytes);
    let at = *offsets
        .iter()
        .find(|&&at| u32_at(&bytes, at + 24) == b"classical-kernel-fixture".len())
        .unwrap();
    let local = u32_at(&bytes, at + 42);
    let start = local + 30 + u16_at(&bytes, local + 26);
    let end = start + u32_at(&bytes, at + 24);
    bytes[start] ^= 1;
    assert_eq!(
        read_fixture(&bytes).unwrap_err().code,
        ErrorCode::IntegrityMismatch
    );
    let crc = crc32(&bytes[start..end]);
    set32(&mut bytes, local + 14, crc);
    set32(&mut bytes, at + 16, crc);
    assert_eq!(
        read_fixture(&bytes).unwrap_err().code,
        ErrorCode::IntegrityMismatch
    );
}

#[test]
fn bundle_must_contain_exact_manifest_closure() {
    let bytes = fixture();
    let offsets = directory(&bytes);
    // Remove a real physical entry and rebase the remaining valid ZIP headers.
    // Both a missing manifest and a missing declared blob must fail, independently
    // of ZIP framing correctness.
    for removed in [0, 1] {
        let mut out = Vec::new();
        let mut records = Vec::new();
        for (index, &at) in offsets.iter().enumerate() {
            if index == removed {
                continue;
            }
            let start = u32_at(&bytes, at + 42);
            let len = 30 + u16_at(&bytes, start + 26) + u32_at(&bytes, at + 24);
            let mut record = bytes[at..at + 46 + u16_at(&bytes, at + 28)].to_vec();
            set32(&mut record, 42, out.len() as u32);
            out.extend_from_slice(&bytes[start..start + len]);
            records.push(record);
        }
        let central_start = out.len();
        for record in records {
            out.extend_from_slice(&record);
        }
        let central_size = out.len() - central_start;
        let mut footer = bytes[bytes.len() - 22..].to_vec();
        set16(&mut footer, 8, offsets.len() as u16 - 1);
        set16(&mut footer, 10, offsets.len() as u16 - 1);
        set32(&mut footer, 12, central_size as u32);
        set32(&mut footer, 16, central_start as u32);
        out.extend_from_slice(&footer);
        let error = read_fixture(&out).unwrap_err();
        assert_eq!(error.code, ErrorCode::Malformed);
        assert!(error.message.contains(if removed == 0 {
            "manifest is absent"
        } else {
            "closure"
        }));
    }
    // A syntactically safe but undeclared blob path also fails. CRC stays valid;
    // changing archive filenames cannot redirect a declared digest.
    let mut renamed = bytes.clone();
    let at = offsets[1];
    let local = u32_at(&bytes, at + 42);
    let old = renamed[at + 46 + 13];
    let replacement = if old == b'0' { b'1' } else { b'0' };
    renamed[at + 46 + 13] = replacement;
    renamed[local + 30 + 13] = replacement;
    let error = read_fixture(&renamed).unwrap_err();
    assert!(error.message.contains("closure"), "{error:?}");
}

#[test]
fn boundaries_are_inclusive_and_refuse_excess() {
    let bytes = fixture();
    let bundle = read_fixture(&bytes).unwrap();
    let count = directory(&bytes).len();
    let exact = BundleLimits {
        max_bytes: bytes.len(),
        max_entries: count,
    };
    assert!(read(&bytes, &Limits::default(), exact).is_ok());
    assert!(
        pack(
            bundle.release().root(),
            bundle.artifacts(),
            &Limits::default(),
            exact
        )
        .is_ok()
    );
    for budget in [
        BundleLimits {
            max_bytes: bytes.len() - 1,
            ..exact
        },
        BundleLimits {
            max_entries: count - 1,
            ..exact
        },
    ] {
        assert_eq!(
            read(&bytes, &Limits::default(), budget).unwrap_err().code,
            ErrorCode::LimitExceeded
        );
        assert_eq!(
            pack(
                bundle.release().root(),
                bundle.artifacts(),
                &Limits::default(),
                budget
            )
            .unwrap_err()
            .code,
            ErrorCode::LimitExceeded
        );
    }
    let limits = Limits {
        max_declared_bytes: 1,
        ..Limits::default()
    };
    assert_eq!(
        read(&bytes, &limits, exact).unwrap_err().code,
        ErrorCode::LimitExceeded
    );
}
