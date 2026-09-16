//! Allocation-light preflight of untrusted JSON before constructing the DTO.
use super::{ContainerLimits, DocumentError, limit, malformed};
use serde::{
    Deserializer,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use std::fmt;

/// JSON work/shape bounds separate from archive bytes and scientific blob sizes.
#[derive(Clone, Copy, Debug)]
pub struct MetadataLimits {
    /// Total values and map keys visited.
    pub values: usize,
    /// Nesting beneath the root; never widened past 64 by this reader.
    pub depth: usize,
    /// One decoded text value or map key, in UTF-8 bytes.
    pub text_bytes: usize,
    /// Maximum items in an otherwise unspecified collection.
    pub collection_items: usize,
    /// Maximum fields in a JSON object.
    pub object_fields: usize,
}
impl Default for MetadataLimits {
    fn default() -> Self {
        Self {
            values: 1_000_000,
            depth: 32,
            text_bytes: 4096,
            collection_items: 65_536,
            object_fields: 64,
        }
    }
}
struct Budget {
    limits: ContainerLimits,
    remaining: usize,
    exceeded: bool,
}
impl Budget {
    fn refuse<E: de::Error>(&mut self) -> E {
        self.exceeded = true;
        E::custom("document metadata budget exceeded")
    }
    fn value<E: de::Error>(&mut self, depth: usize) -> Result<(), E> {
        if depth > self.limits.metadata.depth.min(64) || self.remaining == 0 {
            return Err(self.refuse());
        }
        self.remaining -= 1;
        Ok(())
    }
    fn count(&self, key: &str) -> usize {
        let l = self.limits;
        let specialized = match key {
            "kernels" | "kernelInstances" => l.scientific.kernels,
            "contributions" | "roots" => l.selection.contributions,
            "bindings" => l.selection.bindings,
            "releaseEvidence" => l.selection.releases,
            "variables" => kagami_document::Limits::DEFAULT.max_variables,
            "objects" => kagami_document::Limits::DEFAULT.max_objects,
            "components" => kagami_document::Limits::DEFAULT.max_components_per_object,
            "properties" => kagami_document::Limits::DEFAULT.max_properties_per_component,
            _ => l.metadata.collection_items,
        };
        specialized.min(l.metadata.collection_items)
    }
}
struct Seed<'a> {
    budget: &'a mut Budget,
    depth: usize,
    items: usize,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        self.budget.value(self.depth)?;
        d.deserialize_any(self)
    }
}
// Invoked only when an extra array entry exists; it refuses without decoding
// that entry, including if the entry would otherwise be malformed/expensive.
struct Stop<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for Stop<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
        Err(self.0.refuse())
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded JSON metadata")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: de::Error>(self, s: &str) -> Result<(), E> {
        if s.len() > self.budget.limits.metadata.text_bytes {
            Err(self.budget.refuse())
        } else {
            Ok(())
        }
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        for _ in 0..self.items {
            let items = self.budget.limits.metadata.collection_items;
            if seq
                .next_element_seed(Seed {
                    budget: self.budget,
                    depth: self.depth + 1,
                    items,
                })?
                .is_none()
            {
                return Ok(());
            }
        }
        seq.next_element_seed(Stop(self.budget))?;
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        for _ in 0..self.budget.limits.metadata.object_fields.min(self.items) {
            let Some(key) = map.next_key::<String>()? else {
                return Ok(());
            };
            self.budget.value(self.depth + 1)?;
            if key.len() > self.budget.limits.metadata.text_bytes {
                return Err(self.budget.refuse());
            }
            let items = self.budget.count(&key);
            map.next_value_seed(Seed {
                budget: self.budget,
                depth: self.depth + 1,
                items,
            })?;
        }
        map.next_key_seed(Stop(self.budget))?;
        Ok(())
    }
}
pub(super) fn check(bytes: &[u8], limits: ContainerLimits) -> Result<(), DocumentError> {
    let mut budget = Budget {
        remaining: limits.metadata.values,
        limits,
        exceeded: false,
    };
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let result = Seed {
        budget: &mut budget,
        depth: 0,
        items: limits.metadata.collection_items,
    }
    .deserialize(&mut de)
    .and_then(|()| de.end());
    match result {
        Ok(()) => Ok(()),
        Err(_) if budget.exceeded => Err(limit()),
        Err(error) => Err(malformed(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_before_reading_the_over_limit_entry() {
        let mut limits = ContainerLimits::default();
        limits.scientific.kernels = 1;
        assert!(check(br#"{"kernels":[{}]}"#, limits).is_ok());
        assert_eq!(
            check(br#"{"kernels":[{},this is not JSON]}"#, limits)
                .unwrap_err()
                .code(),
            "scientific_setup_limit"
        );
    }
    #[test]
    fn bounds_depth_nodes_fields_and_text() {
        for limits in [
            MetadataLimits {
                depth: 0,
                ..Default::default()
            },
            MetadataLimits {
                values: 1,
                ..Default::default()
            },
            MetadataLimits {
                text_bytes: 1,
                ..Default::default()
            },
            MetadataLimits {
                object_fields: 0,
                ..Default::default()
            },
        ] {
            assert_eq!(
                check(
                    br#"{"key":"text"}"#,
                    ContainerLimits {
                        metadata: limits,
                        ..Default::default()
                    }
                )
                .unwrap_err()
                .code(),
                "scientific_setup_limit"
            );
        }
    }
}
