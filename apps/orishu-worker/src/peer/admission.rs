//! Credential/source-network evidence for the membership core. No DNS, clocks,
//! or advertised peer addresses participate in source-network authorization.

use crate::credentials::SecretToken;
use orishu_membership::{AdmissionEvidence, NetworkPattern};
use std::net::IpAddr;

const MAX_NETWORK_RULES: usize = 1024;
const MAX_PATTERN_BYTES: usize = 253;

/// Bounded, non-payload-bearing policy diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NetworkError {
    /// A rule is not a literal IP or CIDR with a valid prefix width.
    #[error("invalid source-network rule")]
    Malformed,
    /// The configured rule count or encoded rule length exceeds the profile.
    #[error("source-network policy exceeds its limit")]
    Limit,
}

/// An already parsed, bounded IP/CIDR rule. Host bits in CIDRs are masked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkRule {
    address: IpAddr,
    prefix: u8,
}

impl NetworkRule {
    /// Parse a literal address or CIDR. DNS names, ports, brackets, zones,
    /// whitespace and signed/over-width prefixes are invalid.
    pub fn parse(pattern: &str) -> Result<Self, NetworkError> {
        if pattern.len() > MAX_PATTERN_BYTES {
            return Err(NetworkError::Limit);
        }
        let (address, prefix) = match pattern.split_once('/') {
            Some((address, prefix)) => (address, Some(prefix)),
            None => (pattern, None),
        };
        let address: IpAddr = address.parse().map_err(|_| NetworkError::Malformed)?;
        let width = if address.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(value) if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                value.parse::<u8>().map_err(|_| NetworkError::Malformed)?
            }
            Some(_) => return Err(NetworkError::Malformed),
            None => width,
        };
        if prefix > width {
            return Err(NetworkError::Malformed);
        }
        Ok(Self { address, prefix })
    }

    /// Match the transport-observed address, including IPv4-mapped IPv6 peers.
    /// For IPv6 rules, IPv4 sources use their IPv4-mapped representation; thus a
    /// broad IPv6 rule covering that space also blocks those sources.
    pub fn contains(&self, source: IpAddr) -> bool {
        match self.address {
            IpAddr::V4(network) => {
                let IpAddr::V4(source) = source.to_canonical() else {
                    return false;
                };
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - self.prefix)
                };
                u32::from(network) & mask == u32::from(source) & mask
            }
            IpAddr::V6(network) => {
                let source = match source {
                    IpAddr::V4(source) => source.to_ipv6_mapped(),
                    IpAddr::V6(source) => source,
                };
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - self.prefix)
                };
                u128::from(network) & mask == u128::from(source) & mask
            }
        }
    }
}

/// A verification always has an explicit verdict, including malformed policy.
/// An adapter must deliver `evidence` to the correlated pending core request;
/// it must not drop a completion merely because it also reports a policy error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verification {
    /// Secret-free verdict consumed by the core's admission transition.
    pub evidence: AdmissionEvidence,
    /// Optional diagnostic without rule text, token, or source address.
    pub policy_error: Option<NetworkError>,
}

/// Source-network decision independent of secret-token verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceVerdict {
    /// A matching block or invalid/oversized policy denies the source.
    pub blocked: bool,
    /// Redacted policy parsing/limit failure, if any.
    pub policy_error: Option<NetworkError>,
}

/// Check at most 1,024 active network rules without collecting a temporary list.
/// The source must come from the current transport address, not an advertisement.
pub fn check_source<'a>(
    source: IpAddr,
    blocked: impl IntoIterator<Item = &'a NetworkPattern>,
) -> SourceVerdict {
    let mut verdict = SourceVerdict {
        blocked: false,
        policy_error: None,
    };
    for (index, pattern) in blocked.into_iter().enumerate() {
        if index == MAX_NETWORK_RULES {
            verdict.blocked = true;
            verdict.policy_error = Some(NetworkError::Limit);
            break;
        }
        match NetworkRule::parse(&pattern.0) {
            Ok(rule) => verdict.blocked |= rule.contains(source),
            Err(error) => {
                verdict.blocked = true;
                verdict.policy_error = Some(error);
            }
        }
    }
    verdict
}

/// Validate source-network policy independently of any source address. Used
/// before enabling an introducer, not as an authorization decision for a peer.
pub fn validate_networks<'a>(
    patterns: impl IntoIterator<Item = &'a NetworkPattern>,
) -> Result<(), NetworkError> {
    for (index, pattern) in patterns.into_iter().enumerate() {
        if index == MAX_NETWORK_RULES {
            return Err(NetworkError::Limit);
        }
        NetworkRule::parse(&pattern.0)?;
    }
    Ok(())
}

/// Verify bounded source-network policy and a worker-held formation join token.
/// `source` must be the currently observed QUIC peer address, never an advertised
/// endpoint or proxy header. The caller separately fences session/generation and
/// supplies the correct formation's token. Complexity is O(rules), capped at 1024.
pub fn verify(
    expected: &SecretToken,
    presented: Option<&SecretToken>,
    source: IpAddr,
    authenticated: bool,
    blocked: &[NetworkPattern],
) -> Verification {
    let source = check_source(source, blocked);
    Verification {
        evidence: AdmissionEvidence {
            transport_authenticated: authenticated,
            token_valid: presented.is_some_and(|candidate| expected.matches(candidate.expose())),
            source_network_blocked: source.blocked,
        },
        policy_error: source.policy_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_and_cidr_boundaries_include_mapped_sources() {
        for (rule, source, expected) in [
            ("10.1.2.17/24", "10.1.2.255", true),
            ("10.1.2.0/24", "10.1.3.0", false),
            ("0.0.0.0/0", "255.255.255.255", true),
            ("10.1.2.3", "10.1.2.4", false),
            ("10.1.2.0/24", "::ffff:10.1.2.7", true),
            ("10.1.2.0/24", "2001:db8::1", false),
            ("2001:db8::/32", "2001:db8:ffff::1", true),
            ("2001:db8::/32", "2001:db9::1", false),
            ("::1/128", "::1", true),
            ("::1/128", "::2", false),
            ("::ffff:10.1.2.0/120", "10.1.2.7", true),
            ("::/0", "10.1.2.7", true),
        ] {
            assert_eq!(
                NetworkRule::parse(rule)
                    .unwrap()
                    .contains(source.parse().unwrap()),
                expected,
                "{rule} / {source}"
            );
        }
    }

    #[test]
    fn malformed_policy_is_not_interpreted_as_an_empty_blocklist() {
        let expected = SecretToken::generate().unwrap();
        for rule in [
            "example.com",
            "127.0.0.1:80",
            "[::1]",
            "fe80::1%eth0",
            "10.0.0.0/33",
            "::/129",
            "::/-1",
            "::/+1",
            "::/",
            "::/1/2",
            " ::1",
        ] {
            let verification = verify(
                &expected,
                Some(&expected),
                "127.0.0.1".parse().unwrap(),
                true,
                &[NetworkPattern(rule.into())],
            );
            assert!(verification.evidence.source_network_blocked);
            assert_eq!(verification.policy_error, Some(NetworkError::Malformed));
        }
        let rules = vec![NetworkPattern("127.0.0.1".into()); MAX_NETWORK_RULES + 1];
        assert_eq!(
            verify(
                &expected,
                Some(&expected),
                "127.0.0.1".parse().unwrap(),
                true,
                &rules
            )
            .policy_error,
            Some(NetworkError::Limit)
        );
    }

    #[test]
    fn independent_credentials_and_transport_facts_cannot_substitute_for_each_other() {
        let expected = SecretToken::generate().unwrap();
        let other = SecretToken::generate().unwrap();
        let source = "127.0.0.1".parse().unwrap();
        let accepted = verify(&expected, Some(&expected), source, true, &[]);
        assert!(accepted.evidence.token_valid && accepted.evidence.transport_authenticated);
        assert!(!accepted.evidence.source_network_blocked);
        assert!(
            !verify(&expected, Some(&other), source, true, &[])
                .evidence
                .token_valid
        );
        assert!(
            !verify(&expected, None, source, true, &[])
                .evidence
                .token_valid
        );
        assert!(
            !verify(&expected, Some(&expected), source, false, &[])
                .evidence
                .transport_authenticated
        );
        assert!(
            verify(
                &expected,
                Some(&expected),
                source,
                true,
                &[NetworkPattern("127.0.0.0/8".into())]
            )
            .evidence
            .source_network_blocked
        );
        assert!(!format!("{accepted:?}").contains(expected.expose()));
    }
}
