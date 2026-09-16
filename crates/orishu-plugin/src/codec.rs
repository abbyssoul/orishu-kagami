//! Shared CBOR primitives plus a bounded authoring adapter and explicit projections.
use crate::{projection::Project, *};
use orishu_workload::canonical::{self, CanonicalMap, CanonicalValue as V};
use serde::{
    Deserialize, Deserializer,
    de::{self, DeserializeSeed, IntoDeserializer, MapAccess, SeqAccess, Visitor},
};
use std::fmt;

pub(crate) fn budget(limits: &Limits, bytes: usize) -> orishu_workload::Limits {
    orishu_workload::Limits {
        max_manifest_bytes: bytes as u64,
        max_nesting_depth: limits.max_depth.min(64),
        max_canonical_values: limits.max_values,
        ..orishu_workload::Limits::DEFAULT
    }
}
fn canonical_error(e: canonical::CanonicalError) -> Error {
    use canonical::CanonicalError::*;
    let code = match e {
        TooDeep { .. } | TooManyValues { .. } | TooLarge { .. } | EncodedTooLarge { .. } => {
            ErrorCode::LimitExceeded
        }
        _ => ErrorCode::Malformed,
    };
    // Do not echo arbitrary map keys or codec paths into public diagnostics.
    Error::new(code, "$", "invalid or over-budget canonical encoding")
}
pub(crate) fn encode(value: &V, limits: &Limits, max_bytes: usize) -> Result<Vec<u8>, Error> {
    let mut remaining = limits.max_values;
    check_tree(value, limits, "", 0, &mut remaining)?;
    canonical::encode(value, &budget(limits, max_bytes)).map_err(canonical_error)
}

fn count_limit(key: &str, limits: &Limits) -> usize {
    match key {
        "contributions" => limits.max_contributions,
        "artifacts" => limits.max_artifacts,
        "requirements" => limits.max_requirements,
        "observables" | "requiredObservables" => limits.max_channels,
        "dimension" => 7,
        "axes" => 2,
        _ => limits.max_schema_items,
    }
}
fn limit(path: &str) -> Error {
    Error::new(ErrorCode::LimitExceeded, path, "input budget exceeded")
}
fn check_tree(
    v: &V,
    l: &Limits,
    key: &str,
    depth: usize,
    remaining: &mut usize,
) -> Result<(), Error> {
    if depth > l.max_depth.min(64) {
        return Err(limit(key));
    }
    *remaining = remaining.checked_sub(1).ok_or_else(|| limit(key))?;
    match v {
        V::Text(s) if s.len() > l.max_text_bytes => return Err(limit(key)),
        V::Bytes(_) => {
            return Err(Error::malformed(
                key,
                "byte strings are not declaration values",
            ));
        }
        V::Array(items) => {
            if items.len() > count_limit(key, l) {
                return Err(limit(key));
            }
            for v in items {
                check_tree(v, l, "", depth + 1, remaining)?;
            }
        }
        V::Map(map) => {
            if map.entries().len() > l.max_object_fields {
                return Err(limit(key));
            }
            for (k, v) in map.entries() {
                let V::Text(k) = k else {
                    return Err(Error::malformed(key, "map keys must be text"));
                };
                if depth + 1 > l.max_depth.min(64) || k.len() > l.max_text_bytes {
                    return Err(limit("map key"));
                }
                *remaining = remaining.checked_sub(1).ok_or_else(|| limit("map key"))?;
                check_tree(v, l, k, depth + 1, remaining)?;
            }
        }
        _ => {}
    }
    Ok(())
}

// Borrowed serde adapter: ArtifactDigest's borrowed string reader works here,
// unlike serde_json::from_value. Identity never uses this deserializer backwards.
#[derive(Clone, Copy)]
struct Value<'a>(&'a V);
impl<'de> IntoDeserializer<'de, Error> for Value<'de> {
    type Deserializer = Self;
    fn into_deserializer(self) -> Self {
        self
    }
}
impl de::Error for Error {
    fn custom<T: fmt::Display>(_message: T) -> Self {
        Error::malformed("$", "value does not match the declared schema")
    }
}
impl<'de> Deserializer<'de> for Value<'de> {
    type Error = Error;
    fn deserialize_any<W: Visitor<'de>>(self, visitor: W) -> Result<W::Value, Error> {
        match self.0 {
            V::Bool(v) => visitor.visit_bool(*v),
            V::UInt(v) => visitor.visit_u64(*v),
            V::NInt(v) => visitor.visit_i64(*v),
            V::F64(v) => visitor.visit_f64(v.get()),
            V::Text(v) => visitor.visit_borrowed_str(v),
            V::Bytes(v) => visitor.visit_borrowed_bytes(v),
            V::Array(items) => {
                visitor.visit_seq(de::value::SeqDeserializer::new(items.iter().map(Value)))
            }
            V::Map(map) => visitor.visit_map(de::value::MapDeserializer::new(
                map.entries().iter().map(|(k, v)| (Value(k), Value(v))),
            )),
        }
    }
    fn deserialize_option<W: Visitor<'de>>(self, visitor: W) -> Result<W::Value, Error> {
        visitor.visit_some(self)
    }
    fn deserialize_enum<W: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: W,
    ) -> Result<W::Value, Error> {
        if let V::Text(v) = self.0 {
            visitor.visit_enum(de::value::BorrowedStrDeserializer::<Error>::new(v))
        } else {
            Err(Error::malformed("$", "expected enum spelling"))
        }
    }
    fn deserialize_newtype_struct<W: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: W,
    ) -> Result<W::Value, Error> {
        visitor.visit_newtype_struct(self)
    }
    serde::forward_to_deserialize_any! { bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf unit unit_struct seq tuple tuple_struct map struct identifier ignored_any }
}

struct Seed<'a> {
    limits: &'a Limits,
    depth: usize,
    key: String,
    remaining: &'a mut usize,
    failure: &'a mut Option<Error>,
}
impl Seed<'_> {
    fn fail<E: de::Error>(&mut self) -> E {
        *self.failure = Some(limit(&self.key));
        E::custom("plugin input budget exceeded")
    }
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = V;
    fn deserialize<D: Deserializer<'de>>(mut self, d: D) -> Result<V, D::Error> {
        if self.depth > self.limits.max_depth.min(64) || *self.remaining == 0 {
            return Err(self.fail());
        }
        *self.remaining -= 1;
        d.deserialize_any(self)
    }
}
// A collection can test whether the next entry exists without allowing that
// entry's deserializer to read any key/value bytes.
struct Stop;
impl<'de> DeserializeSeed<'de> for Stop {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
        Err(de::Error::custom("collection limit"))
    }
}

impl<'de> Visitor<'de> for Seed<'_> {
    type Value = V;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded non-null plugin declaration")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<V, E> {
        Ok(V::Bool(v))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<V, E> {
        Ok(V::UInt(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<V, E> {
        Ok(V::int(v))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<V, E> {
        FiniteF64::new(v).map(V::F64).map_err(E::custom)
    }
    fn visit_str<E: de::Error>(mut self, v: &str) -> Result<V, E> {
        if v.len() > self.limits.max_text_bytes {
            return Err(self.fail());
        }
        Ok(V::text(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<V, A::Error> {
        let mut items = Vec::new();
        let max = count_limit(&self.key, self.limits);
        loop {
            if items.len() == max {
                // next_element_seed discovers whether another element exists;
                // Stop never asks its deserializer to read that rejected element.
                match seq.next_element_seed(Stop) {
                    Ok(None) => break,
                    _ => return Err(self.fail()),
                }
            }
            let next = seq.next_element_seed(Seed {
                limits: self.limits,
                depth: self.depth + 1,
                key: String::new(),
                remaining: self.remaining,
                failure: self.failure,
            })?;
            match next {
                Some(v) => items.push(v),
                None => break,
            }
        }
        Ok(V::Array(items))
    }
    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<V, A::Error> {
        let mut entries = Vec::new();
        let mut keys = std::collections::BTreeSet::new();
        loop {
            if entries.len() == self.limits.max_object_fields {
                match map.next_key_seed(Stop) {
                    Ok(None) => break,
                    _ => return Err(self.fail()),
                }
            }
            let Some(k) = map.next_key_seed(Seed {
                limits: self.limits,
                depth: self.depth + 1,
                key: String::new(),
                remaining: self.remaining,
                failure: self.failure,
            })?
            else {
                break;
            };
            let V::Text(key) = k else {
                return Err(de::Error::custom("non-text key"));
            };
            if !keys.insert(key.clone()) {
                return Err(de::Error::custom("duplicate key"));
            }
            let v = map.next_value_seed(Seed {
                limits: self.limits,
                depth: self.depth + 1,
                key: key.clone(),
                remaining: self.remaining,
                failure: self.failure,
            })?;
            entries.push((V::Text(key), v));
        }
        CanonicalMap::new(entries)
            .map(V::Map)
            .map_err(de::Error::custom)
    }
}
fn json(bytes: &[u8], l: &Limits, max: usize) -> Result<V, Error> {
    if bytes.len() > max {
        return Err(limit("$"));
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let mut failure = None;
    let mut remaining = l.max_values;
    let result = Seed {
        limits: l,
        depth: 0,
        key: String::new(),
        remaining: &mut remaining,
        failure: &mut failure,
    }
    .deserialize(&mut de);
    let value = result.map_err(|_| {
        failure.unwrap_or_else(|| Error::malformed("$", "invalid JSON declaration"))
    })?;
    de.end()
        .map_err(|_| Error::malformed("$", "trailing JSON data"))?;
    Ok(value)
}
fn cbor(bytes: &[u8], l: &Limits, max: usize) -> Result<V, Error> {
    let value = canonical::decode(bytes, &budget(l, max)).map_err(canonical_error)?;
    let mut remaining = l.max_values;
    check_tree(&value, l, "", 0, &mut remaining)?;
    Ok(value)
}
/// Reuse the bounded declaration-shaped tree reader for cold execution metadata.
/// Its owner must validate relationships and compare its explicit projection.
pub(crate) fn structured_from_cbor<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    limits: &Limits,
    max: usize,
) -> Result<T, Error> {
    T::deserialize(Value(&cbor(bytes, limits, max)?))
}
fn read_release(v: &V, l: &Limits) -> Result<Release, Error> {
    let release = Release::deserialize(Value(v))?;
    release.validate(l)?;
    Ok(release)
}
fn read_payload(point: KnownPoint, v: &V, l: &Limits) -> Result<Payload, Error> {
    let p = match point {
        KnownPoint::Components => Payload::Components(Deserialize::deserialize(Value(v))?),
        KnownPoint::Fields => Payload::Fields(Deserialize::deserialize(Value(v))?),
        KnownPoint::Observables => Payload::Observables(Deserialize::deserialize(Value(v))?),
        KnownPoint::Constants => Payload::Constants(Deserialize::deserialize(Value(v))?),
        KnownPoint::FieldModels => Payload::FieldModels(Deserialize::deserialize(Value(v))?),
        KnownPoint::Integrators => Payload::Integrators(Deserialize::deserialize(Value(v))?),
    };
    p.validate(l)?;
    Ok(p)
}
/// Decode a bounded JSON root and validate it, without fetching any payload.
pub fn release_from_json(bytes: &[u8], l: &Limits) -> Result<Release, Error> {
    read_release(&json(bytes, l, l.max_manifest_bytes)?, l)
}
/// Decode a canonical root; noncanonical set order and numeric spellings are errors.
pub fn release_from_cbor(bytes: &[u8], l: &Limits) -> Result<Release, Error> {
    let r = read_release(&cbor(bytes, l, l.max_manifest_bytes)?, l)?;
    if r.canonical_bytes(l)? != bytes {
        return Err(Error::malformed("$", "not the root's canonical form"));
    }
    Ok(r)
}
/// Decode one understood JSON payload. Unknown points are verified as opaque bytes.
pub fn payload_from_json(point: KnownPoint, bytes: &[u8], l: &Limits) -> Result<Payload, Error> {
    read_payload(point, &json(bytes, l, l.max_payload_bytes)?, l)
}
/// Decode one understood canonical payload with exact re-encoding verification.
pub fn payload_from_cbor(point: KnownPoint, bytes: &[u8], l: &Limits) -> Result<Payload, Error> {
    let p = read_payload(point, &cbor(bytes, l, l.max_payload_bytes)?, l)?;
    if p.canonical_bytes(l)? != bytes {
        return Err(Error::malformed("$", "not the payload's canonical form"));
    }
    Ok(p)
}
impl Release {
    /// Explicit deterministic root encoding after validation; never fetches bytes.
    pub fn canonical_bytes(&self, l: &Limits) -> Result<Vec<u8>, Error> {
        self.validate(l)?;
        encode(&self.project(), l, l.max_manifest_bytes)
    }
    /// Compute immutable release identity from canonical root bytes.
    pub fn release_id(&self, l: &Limits) -> Result<PluginReleaseId, Error> {
        Ok(PluginReleaseId::of_canonical(&self.canonical_bytes(l)?))
    }
}
impl Payload {
    /// Explicit deterministic full payload encoding, including presentation.
    pub fn canonical_bytes(&self, l: &Limits) -> Result<Vec<u8>, Error> {
        self.validate(l)?;
        encode(&self.project(), l, l.max_payload_bytes)
    }
    /// Encode scientific semantics only, not presentation or provider identity.
    pub fn scientific_bytes(&self, l: &Limits) -> Result<Vec<u8>, Error> {
        self.validate(l)?;
        encode(&self.scientific_projection(), l, l.max_payload_bytes)
    }
    /// Scientific identity; changing defaults/dependency contracts changes its digest.
    pub fn contract_ref(&self, l: &Limits) -> Result<ScientificContractRef, Error> {
        let digest = ScientificDigest::of_canonical(&self.scientific_bytes(l)?);
        Ok(
            crate::projection::dispatch!(self,v=>ScientificContractRef{name:v.scientific.name.clone(),version:v.scientific.version,digest}),
        )
    }
}
