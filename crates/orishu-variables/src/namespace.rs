use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// What is wrong with a candidate [`Name`] segment.
///
/// A name must be non-empty, must not start with a digit, and may only
/// contain ASCII alphanumerics and underscores (no spaces, punctuation, or
/// the `.` namespace path separator).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum InvalidName {
    /// The candidate text was empty.
    #[error("name must not be empty")]
    Empty,
    /// The candidate text started with a digit.
    #[error("name must not start with a digit: `{0}`")]
    StartsWithDigit(String),
    /// The candidate text contained a character other than an ASCII
    /// alphanumeric or underscore.
    #[error("name `{0}` contains invalid character `{1}`")]
    InvalidCharacter(String, char),
}

fn validate_name(value: &str) -> Result<(), InvalidName> {
    let mut chars = value.chars();
    let first = chars.next().ok_or(InvalidName::Empty)?;
    if first.is_ascii_digit() {
        return Err(InvalidName::StartsWithDigit(value.to_owned()));
    }
    if !(first.is_alphabetic() || first == '_') {
        return Err(InvalidName::InvalidCharacter(value.to_owned(), first));
    }
    for c in chars {
        if !(c.is_alphanumeric() || c == '_') {
            return Err(InvalidName::InvalidCharacter(value.to_owned(), c));
        }
    }
    Ok(())
}

/// A variable name, unique within its namespace.
///
/// A name is never empty, never starts with a digit, and never contains
/// spaces or the `.` namespace path separator.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Name(String);

impl Name {
    /// Validate and construct a [`Name`] from its text.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidName> {
        let value = value.into();
        validate_name(&value)?;
        Ok(Self(value))
    }

    /// Read the underlying name text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<&str> for Name {
    type Error = InvalidName;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for Name {
    type Error = InvalidName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Accepts either an already-validated [`Name`] or raw text to validate,
/// letting APIs like [`crate::VariablesSystem::define`] take a `&str`
/// literal or a `Name` interchangeably.
pub trait IntoName {
    /// Validate (if needed) and produce the [`Name`].
    fn into_name(self) -> Result<Name, InvalidName>;
}

impl IntoName for Name {
    fn into_name(self) -> Result<Name, InvalidName> {
        Ok(self)
    }
}

impl IntoName for &str {
    fn into_name(self) -> Result<Name, InvalidName> {
        Name::new(self)
    }
}

impl IntoName for String {
    fn into_name(self) -> Result<Name, InvalidName> {
        Name::new(self)
    }
}

impl PartialEq<str> for Name {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for Name {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// A dotted namespace path, e.g. `"globals.electricity"`.
///
/// Namespaces are flat strings: hierarchy is expressed only through shared
/// dotted prefixes, there is no separate tree structure. The empty string is
/// the root namespace. A variable's fully qualified name is its namespace
/// joined with its own name by a `.`, e.g. `"globals.electricity.K"`.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Namespace(String);

impl Namespace {
    /// Construct a new namespace from its dotted path text.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Read the dotted path text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` for the root namespace (the empty path), which bare
    /// (undotted) symbol references resolve in.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// Build a fully qualified name from this namespace and a trailing name
    /// segment.
    pub fn qualified(&self, name: &Name) -> FQName {
        FQName::new(self.clone(), name.clone())
    }
}

impl From<&str> for Namespace {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Namespace {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A fully qualified name: the unique identifier of a variable, combining
/// its [`Namespace`] (possibly the root namespace) and its [`Name`] within
/// that namespace.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FQName {
    namespace: Namespace,
    name: Name,
}

impl FQName {
    /// Build a fully qualified name from an already-split namespace and name.
    pub fn new(namespace: Namespace, name: Name) -> Self {
        Self { namespace, name }
    }

    /// Parse a dotted `"namespace.name"` (or bare `"name"`, resolved in the
    /// root namespace) string, validating the trailing name segment.
    pub fn parse(raw: &str) -> Result<Self, InvalidName> {
        match raw.rsplit_once('.') {
            Some((namespace, name)) => Ok(Self::new(Namespace::new(namespace), Name::new(name)?)),
            None => Ok(Self::new(Namespace::default(), Name::new(raw)?)),
        }
    }

    /// This name's own (unqualified) name segment.
    pub fn name(&self) -> &Name {
        &self.name
    }

    /// This name's namespace. The root namespace when unqualified.
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
}

impl TryFrom<&str> for FQName {
    type Error = InvalidName;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<String> for FQName {
    type Error = InvalidName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl fmt::Display for FQName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.namespace.is_root() {
            write!(formatter, "{}", self.name)
        } else {
            write!(formatter, "{}.{}", self.namespace, self.name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_names() {
        assert_eq!(Name::new("name").unwrap().as_str(), "name");
        assert_eq!(Name::new("name1").unwrap().as_str(), "name1");
        assert_eq!(Name::new("_private").unwrap().as_str(), "_private");
    }

    #[test]
    fn invalid_names() {
        assert_eq!(Name::new(""), Err(InvalidName::Empty));
        assert!(Name::new("1").is_err());
        assert!(Name::new("-name").is_err());
        assert!(Name::new("+name").is_err());
        assert!(Name::new("zname-").is_err());
        assert!(Name::new("zname+").is_err());
        assert!(Name::new("(some)").is_err());
        assert!(Name::new("fq.name").is_err());
        assert!(Name::new("!aaa").is_err());
        assert!(Name::new("some name").is_err());
        assert!(Name::new("some-name").is_err());
        assert!(Name::new("other,name").is_err());
    }

    #[test]
    fn invalid_name_starting_with_digit_reports_the_offending_text() {
        assert_eq!(
            Name::new("1abc"),
            Err(InvalidName::StartsWithDigit("1abc".to_owned()))
        );
    }

    #[test]
    fn invalid_name_reports_the_offending_character() {
        assert_eq!(
            Name::new("bad name"),
            Err(InvalidName::InvalidCharacter("bad name".to_owned(), ' '))
        );
    }

    #[test]
    fn name_try_from_str_and_string() {
        assert_eq!(Name::try_from("K").unwrap().as_str(), "K");
        assert_eq!(Name::try_from(String::from("K")).unwrap().as_str(), "K");
        assert!(Name::try_from("1bad").is_err());
    }

    #[test]
    fn name_equals_matching_str() {
        let name = Name::new("K").unwrap();
        assert_eq!(name, "K");
        assert_eq!(name, *"K");
    }

    #[test]
    fn name_displays_as_its_text() {
        assert_eq!(Name::new("K").unwrap().to_string(), "K");
    }

    #[test]
    fn name_ordering_is_lexicographic() {
        assert!(Name::new("a").unwrap() < Name::new("b").unwrap());
    }

    #[test]
    fn as_str_returns_the_authored_dotted_path() {
        let namespace = Namespace::new("globals.electricity");
        assert_eq!(namespace.as_str(), "globals.electricity");
    }

    #[test]
    fn root_namespace_is_the_empty_string() {
        assert!(Namespace::new("").is_root());
        assert!(Namespace::default().is_root());
        assert!(!Namespace::new("globals").is_root());
    }

    #[test]
    fn namespace_qualified_builds_a_fully_qualified_name() {
        let namespace = Namespace::new("globals.electricity");
        let name = Name::new("K").unwrap();
        let fq = namespace.qualified(&name);
        assert_eq!(fq.namespace(), &namespace);
        assert_eq!(fq.name(), &name);
        assert_eq!(fq.to_string(), "globals.electricity.K");
    }

    #[test]
    fn root_namespace_qualified_name_has_no_leading_dot() {
        let namespace = Namespace::new("");
        let name = Name::new("a").unwrap();
        assert_eq!(namespace.qualified(&name).to_string(), "a");
    }

    #[test]
    fn fqname_parse_splits_at_the_last_dot_only() {
        let fq = FQName::parse("a.b.c").unwrap();
        assert_eq!(fq.namespace().as_str(), "a.b");
        assert_eq!(fq.name().as_str(), "c");
    }

    #[test]
    fn fqname_parse_bare_name_lands_in_root_namespace() {
        let fq = FQName::parse("a").unwrap();
        assert!(fq.namespace().is_root());
        assert_eq!(fq.name().as_str(), "a");
    }

    #[test]
    fn fqname_parse_dotted_name_splits_namespace_and_name() {
        let fq = FQName::parse("globals.electricity.K").unwrap();
        assert_eq!(fq.namespace().as_str(), "globals.electricity");
        assert_eq!(fq.name().as_str(), "K");
    }

    #[test]
    fn fqname_parse_rejects_an_invalid_trailing_name() {
        assert!(FQName::parse("globals.1bad").is_err());
        assert!(FQName::parse("1bad").is_err());
    }

    #[test]
    fn fqname_try_from_str_and_string() {
        assert_eq!(
            FQName::try_from("globals.K").unwrap(),
            FQName::parse("globals.K").unwrap()
        );
        assert_eq!(
            FQName::try_from(String::from("globals.K")).unwrap(),
            FQName::parse("globals.K").unwrap()
        );
    }

    #[test]
    fn fqname_display_round_trips_through_parse() {
        let fq = FQName::parse("globals.electricity.K").unwrap();
        assert_eq!(FQName::parse(&fq.to_string()).unwrap(), fq);
    }

    #[test]
    fn fqname_equality_and_ordering() {
        let a = FQName::parse("globals.a").unwrap();
        let b = FQName::parse("globals.b").unwrap();
        assert_eq!(a.clone(), FQName::parse("globals.a").unwrap());
        assert!(a < b);
    }

    #[test]
    fn serde_round_trip_name_namespace_and_fqname() {
        let name = Name::new("K").unwrap();
        let json = serde_json_like_round_trip_name(&name);
        assert_eq!(json, name);

        let namespace = Namespace::new("globals.electricity");
        assert_eq!(
            serde_json::to_string(&namespace).unwrap(),
            "\"globals.electricity\""
        );
        let restored: Namespace = serde_json::from_str("\"globals.electricity\"").unwrap();
        assert_eq!(restored, namespace);
    }

    fn serde_json_like_round_trip_name(name: &Name) -> Name {
        let encoded = serde_json::to_string(name).unwrap();
        assert_eq!(encoded, "\"K\"");
        serde_json::from_str(&encoded).unwrap()
    }
}
