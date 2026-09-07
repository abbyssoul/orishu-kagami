//! Explicit collector credentials, never worker/operator identity fallback.
use super::{DeliveryError, HttpDelivery};
use crate::trace_destination::TraceEndpoint;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::{path::Path, sync::Arc, time::Duration};

/// Borrowed startup paths. Loading is explicit; constructing this value does no IO.
/// No Debug implementation: paths and credential material are not diagnostics.
#[derive(Default)]
pub struct CollectorFiles<'a> {
    /// Explicit collector trust roots; otherwise use the supported system bundle.
    pub ca: Option<&'a Path>,
    /// Optional PEM client chain, paired with `key`.
    pub certificate: Option<&'a Path>,
    /// Optional PEM private key, paired with `certificate`.
    pub key: Option<&'a Path>,
    /// Optional private bearer-token file; one terminal LF/CRLF is accepted.
    pub token: Option<&'a Path>,
}

impl CollectorFiles<'_> {
    /// Prepare a collector transport without connecting or modifying files.
    /// Explicit roots replace default trust; no ambient environment override or
    /// credential fallback is used. Call only after enabled startup validation.
    /// Unix paths are walked without symlinks; unsupported platforms fail closed.
    pub fn prepare(
        &self,
        endpoint: &TraceEndpoint,
        timeout: Duration,
        response_bytes: usize,
    ) -> Result<HttpDelivery, DeliveryError> {
        let invalid = || DeliveryError::Configuration;
        let uri = endpoint.validated_uri().map_err(|_| invalid())?;
        if self.certificate.is_some() != self.key.is_some() {
            return Err(invalid());
        }
        if uri.scheme_str() != Some("https") {
            if self.ca.is_some() || self.certificate.is_some() || self.token.is_some() {
                return Err(invalid());
            }
            return HttpDelivery::new(endpoint, None, None, timeout, response_bytes);
        }
        let roots = match self.ca {
            Some(path) => root_store(&read_file(path, 65_536, false)?, 64)?,
            None => system_roots()?,
        };
        let builder = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| invalid())?
        .with_root_certificates(roots);
        let tls = if let (Some(certificate), Some(key)) = (self.certificate, self.key) {
            let chain = certificates(&read_file(certificate, 65_536, false)?, 8)?;
            let bytes = read_file(key, 16_384, true)?;
            let mut keys = <(rustls::pki_types::pem::SectionKind, Vec<u8>)>::pem_slice_iter(&bytes);
            let (kind, der) = keys.next().ok_or_else(invalid)?.map_err(|_| invalid())?;
            let key = PrivateKeyDer::from_pem(kind, der).ok_or_else(invalid)?;
            if keys.next().is_some() {
                return Err(invalid());
            }
            builder
                .with_client_auth_cert(chain, key)
                .map_err(|_| invalid())?
        } else {
            builder.with_no_client_auth()
        };
        let token = self
            .token
            .map(|path| read_file(path, 4098, true))
            .transpose()?;
        let token = token
            .as_deref()
            .map(|bytes| {
                let bytes = bytes
                    .strip_suffix(b"\r\n")
                    .or_else(|| bytes.strip_suffix(b"\n"))
                    .unwrap_or(bytes);
                std::str::from_utf8(bytes).map_err(|_| invalid())
            })
            .transpose()?;
        HttpDelivery::new(endpoint, Some(tls), token, timeout, response_bytes)
    }
}

fn root_store(bytes: &[u8], maximum: usize) -> Result<rustls::RootCertStore, DeliveryError> {
    let mut roots = rustls::RootCertStore::empty();
    for cert in certificates(bytes, maximum)? {
        roots.add(cert).map_err(|_| DeliveryError::Configuration)?;
    }
    Ok(roots)
}

/// Supported Linux ca-certificates layout. Never enumerate directories, consult
/// SSL_CERT_* variables, or fall back to an unrelated trust source on failure.
#[cfg(target_os = "linux")]
fn system_roots() -> Result<rustls::RootCertStore, DeliveryError> {
    let bytes = read_file_owned(
        Path::new("/etc/ssl/certs/ca-certificates.crt"),
        1_048_576,
        false,
        true,
    )?;
    root_store(&bytes, 1024)
}

#[cfg(not(target_os = "linux"))]
fn system_roots() -> Result<rustls::RootCertStore, DeliveryError> {
    Err(DeliveryError::Configuration)
}

fn certificates(
    bytes: &[u8],
    maximum: usize,
) -> Result<Vec<CertificateDer<'static>>, DeliveryError> {
    let mut certificates = Vec::new();
    for section in <(rustls::pki_types::pem::SectionKind, Vec<u8>)>::pem_slice_iter(bytes) {
        if certificates.len() == maximum {
            return Err(DeliveryError::Configuration);
        }
        let (kind, der) = section.map_err(|_| DeliveryError::Configuration)?;
        certificates.push(CertificateDer::from_pem(kind, der).ok_or(DeliveryError::Configuration)?);
    }
    if certificates.is_empty() {
        return Err(DeliveryError::Configuration);
    }
    Ok(certificates)
}

#[cfg(unix)]
fn read_file(path: &Path, maximum: usize, private: bool) -> Result<Vec<u8>, DeliveryError> {
    read_file_owned(path, maximum, private, false)
}

#[cfg(unix)]
fn read_file_owned(
    path: &Path,
    maximum: usize,
    private: bool,
    system: bool,
) -> Result<Vec<u8>, DeliveryError> {
    use rustix::fs::{FileType, Mode, OFlags, fstat, open, openat};
    use std::{io::Read, path::Component};
    let invalid = || DeliveryError::Configuration;
    if path.as_os_str().is_empty() || path.as_os_str().as_encoded_bytes().len() > 4096 {
        return Err(invalid());
    }
    let directory_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut directory = open(
        if path.is_absolute() { "/" } else { "." },
        directory_flags,
        Mode::empty(),
    )
    .map_err(|_| invalid())?;
    let mut components = path.components().peekable();
    let mut count = 0;
    while let Some(component) = components.next() {
        let stat = fstat(&directory).map_err(|_| invalid())?;
        // A root/worker-owned sticky directory (e.g. /tmp) protects entries
        // against replacement by other users; ordinary writable ancestors do not.
        if (stat.st_uid != 0 && (system || stat.st_uid != rustix::process::geteuid().as_raw()))
            || (stat.st_mode & 0o022 != 0 && stat.st_mode & 0o1000 == 0)
        {
            return Err(invalid());
        }
        count += 1;
        if count > 128 {
            return Err(invalid());
        }
        let name = match component {
            Component::RootDir | Component::CurDir => continue,
            Component::Normal(name) => name,
            _ => return Err(invalid()),
        };
        if components.peek().is_some() {
            directory =
                openat(&directory, name, directory_flags, Mode::empty()).map_err(|_| invalid())?;
            continue;
        }
        let fd = openat(
            &directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| invalid())?;
        let stat = fstat(&fd).map_err(|_| invalid())?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
            || stat.st_nlink != 1
            || (stat.st_uid != 0 && (system || stat.st_uid != rustix::process::geteuid().as_raw()))
            || stat.st_mode & if private { 0o077 } else { 0o022 } != 0
            || stat.st_size < 0
            || stat.st_size as u64 > maximum as u64
        {
            return Err(invalid());
        }
        let mut file = std::fs::File::from(fd);
        // Fixed allocation, including one sentinel byte to detect growth after stat.
        let mut bytes = vec![0; maximum + 1];
        let mut used = 0;
        while used < bytes.len() {
            let read = file.read(&mut bytes[used..]).map_err(|_| invalid())?;
            if read == 0 {
                break;
            }
            used += read;
        }
        if used > maximum {
            return Err(invalid());
        }
        bytes.truncate(used);
        return Ok(bytes);
    }
    Err(invalid())
}

#[cfg(not(unix))]
fn read_file(_path: &Path, _maximum: usize, _private: bool) -> Result<Vec<u8>, DeliveryError> {
    Err(DeliveryError::Configuration)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn write(path: &Path, bytes: &[u8], mode: u32) {
        std::fs::write(path, bytes).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn file_bounds_permissions_links_and_ancestors_are_checked() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = temp.path().join("credential");
        write(&path, b"1234", 0o600);
        assert_eq!(read_file(&path, 4, true).unwrap(), b"1234");
        if rustix::process::geteuid().as_raw() != 0 {
            assert!(read_file_owned(&path, 4, false, true).is_err());
        }
        assert!(read_file(&path, 3, true).is_err());
        write(&path, b"1234", 0o644);
        assert!(read_file(&path, 4, true).is_err());
        assert!(read_file(&path, 4, false).is_ok());
        write(&path, b"1234", 0o666);
        assert!(read_file(&path, 4, false).is_err());
        write(&path, b"1234", 0o600);
        let link = temp.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(read_file(&link, 4, true).is_err());
        std::fs::remove_file(&link).unwrap();
        std::fs::hard_link(&path, &link).unwrap();
        assert!(read_file(&path, 4, true).is_err());
        std::fs::remove_file(&link).unwrap();
        symlink(temp.path(), &link).unwrap();
        assert!(read_file(&link.join("credential"), 4, true).is_err());
        assert!(read_file(temp.path(), 4, true).is_err());
        let fifo = temp.path().join("fifo");
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::Mode::from_raw_mode(0o600),
        )
        .unwrap();
        // No writer is opened: a blocking credential open would hang here.
        assert!(read_file(&fifo, 4, true).is_err());
        assert!(read_file(&temp.path().join("../credential"), 4, true).is_err());
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(read_file(&path, 4, true).is_err());
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn default_bundle_count_and_byte_limits_remain_bounded() {
        let generated = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let pem = generated.cert.pem();
        assert!(root_store(pem.repeat(1024).as_bytes(), 1024).is_ok());
        assert!(root_store(pem.repeat(1025).as_bytes(), 1024).is_err());
        assert!(root_store(b"invalid bundle", 1024).is_err());
        let temp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = temp.path().join("bundle");
        write(&path, &vec![b'x'; 1_048_576], 0o644);
        assert_eq!(read_file(&path, 1_048_576, false).unwrap().len(), 1_048_576);
        write(&path, &vec![b'x'; 1_048_577], 0o644);
        assert!(read_file(&path, 1_048_576, false).is_err());
    }

    #[tokio::test]
    async fn pem_key_pairing_token_and_certificate_limits_are_checked() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let ca = temp.path().join("ca.pem");
        let certificate = temp.path().join("client.pem");
        let key = temp.path().join("client.key");
        let token = temp.path().join("token");
        let generated = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        write(&ca, generated.cert.pem().as_bytes(), 0o644);
        write(&certificate, generated.cert.pem().as_bytes(), 0o644);
        write(
            &key,
            generated.signing_key.serialize_pem().as_bytes(),
            0o600,
        );
        write(&token, b"collector-token\r\n", 0o600);
        let files = CollectorFiles {
            ca: Some(&ca),
            certificate: Some(&certificate),
            key: Some(&key),
            token: Some(&token),
        };
        let endpoint = "https://127.0.0.1:4318/v1/traces".parse().unwrap();
        let prepare = || files.prepare(&endpoint, Duration::from_secs(1), 1024);
        assert!(prepare().is_ok());
        for bytes in [
            b"".as_slice(),
            b"token\nheader: secret",
            b"token\n\n",
            &[0xff],
            &vec![b'a'; 4099],
        ] {
            write(&token, bytes, 0o600);
            assert!(matches!(prepare(), Err(DeliveryError::Configuration)));
        }
        write(&token, b"collector-token\n", 0o600);
        let other = rcgen::KeyPair::generate().unwrap().serialize_pem();
        write(&key, other.as_bytes(), 0o600);
        assert!(prepare().is_err());
        let valid_key = generated.signing_key.serialize_pem();
        write(&key, format!("{valid_key}{valid_key}").as_bytes(), 0o600);
        assert!(prepare().is_err());
        write(&key, valid_key.as_bytes(), 0o600);
        for pem in [
            "not PEM".to_owned(),
            generated.cert.pem().repeat(9),
            valid_key,
        ] {
            write(&certificate, pem.as_bytes(), 0o644);
            assert!(prepare().is_err());
        }
        assert!(certificates(generated.cert.pem().repeat(64).as_bytes(), 64).is_ok());
        assert!(certificates(generated.cert.pem().repeat(65).as_bytes(), 64).is_err());
    }
}
