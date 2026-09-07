//! Shared startup and export destination validation, independent of exporter features.
use serde::Deserialize;

/// Staged operator input. Debug output is redacted even before validation;
/// validation happens after overlays, never in a CLI error that echoes input.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct TraceEndpoint(String);

impl std::fmt::Debug for TraceEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TraceEndpoint([redacted])")
    }
}

impl std::str::FromStr for TraceEndpoint {
    type Err = std::convert::Infallible;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(value.to_owned()))
    }
}

impl TraceEndpoint {
    /// Return a validated URI for transport construction. No raw-input accessor.
    pub fn validated_uri(&self) -> Result<salvo::http::uri::Uri, String> {
        self.validate()?;
        self.0
            .parse()
            .map_err(|_| "invalid tracing endpoint".to_owned())
    }

    /// Validate the destination without DNS or transport IO; errors omit its input.
    pub fn validate(&self) -> Result<(), String> {
        let invalid = || {
            "tracing.endpoint requires an explicit credential-free HTTPS URL (HTTP only for literal loopback IPs), port 1..=65535 if supplied, and a query-free absolute path; maximum 2048 ASCII bytes".to_owned()
        };
        if self.0.len() > 2048
            || !self.0.is_ascii()
            || self
                .0
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
            || self.0.contains(['@', '?', '#', '\\', '%'])
        {
            return Err(invalid());
        }
        let uri = self
            .0
            .parse::<salvo::http::uri::Uri>()
            .map_err(|_| invalid())?;
        let authority = uri.authority().ok_or_else(invalid)?;
        let host = authority.host();
        let suffix = authority.as_str().strip_prefix(host).ok_or_else(invalid)?;
        if !suffix.is_empty()
            && suffix.strip_prefix(':').is_none_or(|port| {
                !port.bytes().all(|byte| byte.is_ascii_digit())
                    || !port.parse::<u16>().is_ok_and(|port| port > 0)
            })
        {
            return Err(invalid());
        }
        let ip = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .ok();
        if !matches!(uri.scheme_str(), Some("https"))
            && !(uri.scheme_str() == Some("http") && ip.is_some_and(|ip| ip.is_loopback()))
        {
            return Err(invalid());
        }
        if host.is_empty()
            || authority.as_str().ends_with(':')
            || uri.path() == "/"
            || !uri.path().starts_with('/')
            || uri.path().split('/').any(|part| matches!(part, "." | ".."))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
