use orishu_plugin::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn payload(point: KnownPoint, source: Value) -> Payload {
    payload_from_json(
        point,
        &serde_json::to_vec(&source).unwrap(),
        &Limits::default(),
    )
    .unwrap()
}
pub fn base(name: &str) -> Value {
    json!({"name":name,"version":1,"requirements":[]})
}
pub fn declaration(mut base: Value, fields: Value) -> Value {
    base.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    json!({"scientific":base})
}
pub fn requirement(slot: &str, p: &Payload) -> Value {
    json!({"slot":slot,"contract":p.contract_ref(&Limits::default()).unwrap()})
}

/// Scientific fixture kernels are deliberately inert bytes, not executable Wasm.
/// Declaration validation must never be advertised as ABI/runtime validation.
pub fn declarations() -> Vec<(&'static str, Payload)> {
    let mass = payload(
        KnownPoint::Components,
        declaration(
            base("org.example.gravity.mass"),
            json!({
                "properties":[{"id":"mass","required":true,"schema":{"kind":"quantity","dimension":[0,1,0,0,0,0,0],"defaultExpression":"1 kg"}}],
                "role":"field-coupling","bindings":{"source":"mass","response":"mass"}
            }),
        ),
    );
    let dynamics = payload(
        KnownPoint::Components,
        declaration(
            base("org.example.dynamics"),
            json!({
                "properties":[{"id":"mass","required":true,"schema":{"kind":"quantity","dimension":[0,1,0,0,0,0,0],"minimumSI":0.0}}],
                "role":"dynamics","bindings":{"inertialMass":"mass"}
            }),
        ),
    );
    let acceleration = payload(
        KnownPoint::Observables,
        declaration(
            base("org.example.gravity.acceleration"),
            json!({
                "meaning":"Gravitational acceleration","shape":{"kind":"vector","length":3},"dimension":[1,0,-2,0,0,0,0],
                "frame":"world","axes":["x,y,z"],"conventions":"Cartesian SI components"
            }),
        ),
    );
    let mut b = base("org.example.gravity");
    b["requirements"] = json!([requirement("acceleration", &acceleration)]);
    let field = payload(
        KnownPoint::Fields,
        declaration(
            b,
            json!({"domainDimension":3,"domainRequirements":["box"],"requiredObservables":["acceleration"]}),
        ),
    );
    let constants = payload(
        KnownPoint::Constants,
        declaration(
            base("org.example.gravity.constants"),
            json!({"constants":[{
                "id":"g","dimension":[3,-1,-2,0,0,0,0],"valueSI":6.67430e-11,"meaning":"Newtonian gravitational constant"
            }]}),
        ),
    );
    let mut b = base("org.example.gravity.classical");
    b["requirements"] = json!([
        requirement("field", &field),
        requirement("mass", &mass),
        requirement("acceleration", &acceleration)
    ]);
    let model = payload(
        KnownPoint::FieldModels,
        declaration(
            b,
            json!({
                "field":"field","couplings":["mass"],"configuration":[],"stateFormat":{"id":"org.example.classical.state","version":1},
                "kernel":ArtifactDigest::sha256_of(b"classical-kernel-fixture"),"executionContract":"orishu:simulation/field@1",
                "observables":["acceleration"],"profile":"orishu.force-then-integrate/v1","fieldTimeConvention":"advanced field, committed entity kinematics","maxStateBytes":1024
            }),
        ),
    );
    let mut b = base("org.example.euler");
    b["requirements"] = json!([requirement("dynamics", &dynamics)]);
    let integrator = payload(
        KnownPoint::Integrators,
        declaration(
            b,
            json!({
                "dynamics":"dynamics","configuration":[],"kernel":ArtifactDigest::sha256_of(b"euler-kernel-fixture"),
                "executionContract":"orishu:simulation/dynamics@1","profile":"orishu.force-then-integrate/v1",
                "history":{"format":{"id":"org.example.euler.history","version":1},"samples":0,"maxBytesPerEntity":0,"coldStart":"memoryless"}
            }),
        ),
    );
    vec![
        ("mass", mass),
        ("dynamics", dynamics),
        ("acceleration", acceleration),
        ("gravity", field),
        ("constants", constants),
        ("classical", model),
        ("euler", integrator),
    ]
}
pub fn release(
    name: &str,
    items: &[(&str, Payload)],
    extra: &[&[u8]],
) -> (Release, BTreeMap<ArtifactDigest, Vec<u8>>) {
    let mut blobs = BTreeMap::new();
    let contributions = items
        .iter()
        .map(|(id, p)| {
            let bytes = p.canonical_bytes(&Limits::default()).unwrap();
            let digest = ArtifactDigest::sha256_of(&bytes);
            blobs.insert(digest, bytes);
            Contribution {
                local_id: LocalContributionId::new(id).unwrap(),
                extension_point: ExtensionPointId::new(p.point().as_str()).unwrap(),
                payload: digest,
                requirements: p
                    .requirements()
                    .iter()
                    .map(|r| Requirement::Contract {
                        slot: r.slot.clone(),
                        contract: r.contract.clone(),
                    })
                    .collect(),
                annotations: None,
            }
        })
        .collect();
    for bytes in extra {
        blobs.insert(ArtifactDigest::sha256_of(bytes), bytes.to_vec());
    }
    let artifacts = blobs
        .iter()
        .map(|(digest, bytes)| Artifact {
            digest: *digest,
            size_bytes: bytes.len() as u64,
            media_type: "application/octet-stream".into(),
        })
        .collect();
    let r = Release::new(
        ReleaseMetadata {
            plugin_id: PluginId::new(name).unwrap(),
            version_label: "v1-test".into(),
            display_name: None,
            description: None,
        },
        ReleaseSpec {
            contributions,
            artifacts,
        },
    );
    (r, blobs)
}
pub fn borrowed(blobs: &BTreeMap<ArtifactDigest, Vec<u8>>) -> BTreeMap<ArtifactDigest, &[u8]> {
    blobs.iter().map(|(k, v)| (*k, v.as_slice())).collect()
}
