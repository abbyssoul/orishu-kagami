mod support;

use orishu_plugin::{resolution::*, selected::*, *};
use std::collections::BTreeMap;
use support::*;

struct Source {
    releases: Vec<VerifiedRelease>,
    blobs: BTreeMap<ArtifactDigest, Vec<u8>>,
    selection: Selection,
    instances: Vec<SelectedKernel>,
}
impl Source {
    fn compile(&self) -> Result<CompiledSelection, Error> {
        compile(
            &self.selection,
            &self.instances,
            &self.releases.iter().collect::<Vec<_>>(),
            &borrowed(&self.blobs),
            SelectionLimits::default(),
        )
    }
    fn exported(&self) -> (SelectionDescriptor, BTreeMap<ArtifactDigest, Vec<u8>>) {
        let compiled = self.compile().unwrap();
        let bytes = compiled
            .verified()
            .artifacts()
            .keys()
            .map(|d| {
                (
                    *d,
                    compiled
                        .evidence()
                        .get(d)
                        .or_else(|| self.blobs.get(d))
                        .unwrap()
                        .clone(),
                )
            })
            .collect();
        let descriptor =
            SelectionDescriptor::from_cbor(compiled.descriptor_bytes(), SelectionLimits::default())
                .unwrap();
        (descriptor, bytes)
    }
}
fn source(edit: impl FnOnce(&mut Vec<(&str, Payload)>)) -> Source {
    let mut d = declarations();
    edit(&mut d);
    let mut vocabulary = d[..5].to_vec();
    // A contributes an unused executable too. Independent solver B is chosen.
    vocabulary.push(d[6].clone());
    let (mut a, mut blobs) = release(
        "org.example.vocabulary",
        &vocabulary,
        &[
            b"euler-kernel-fixture",
            b"classical-kernel-fixture",
            b"unselected-icon",
            b"not even valid CBOR",
        ],
    );
    let opaque = ArtifactDigest::sha256_of(b"not even valid CBOR");
    a.0.spec.contributions.push(Contribution {
        local_id: "future".parse().unwrap(),
        extension_point: "org.example.future/v7".parse().unwrap(),
        payload: opaque,
        requirements: vec![],
        annotations: None,
    });
    // A local dependency cannot be rebound to a same-contract external provider.
    a.0.spec
        .contributions
        .iter_mut()
        .find(|c| c.local_id.as_str() == "gravity")
        .unwrap()
        .requirements[0] = Requirement::Local {
        slot: "acceleration".parse().unwrap(),
        local_contribution: "acceleration".parse().unwrap(),
    };
    let a = VerifiedRelease::verify(a, &borrowed(&blobs), &Limits::default()).unwrap();
    let (b, b_blobs) = release(
        "org.example.solver",
        &d[5..6],
        &[b"classical-kernel-fixture"],
    );
    let b = VerifiedRelease::verify(b, &borrowed(&b_blobs), &Limits::default()).unwrap();
    blobs.extend(b_blobs);
    let entries = [&a, &b].map(|release| InventoryEntry {
        release,
        enabled: true,
        is_default: true,
    });
    let inventory = Inventory::new(1, &entries, Default::default()).unwrap();
    let solver = b.contribution_ref(&"classical".parse().unwrap()).unwrap();
    let ResolutionOutcome::Resolved { selection, .. } = inventory
        .resolve(&ResolutionRequest {
            expected_inventory_revision: 1,
            roots: vec![solver.clone()],
            bindings: vec![],
        })
        .unwrap()
    else {
        panic!("fixture must resolve")
    };
    Source {
        releases: vec![a, b],
        blobs,
        selection,
        instances: vec![SelectedKernel {
            instance_id: "gravity-1".parse().unwrap(),
            contribution: solver,
            execution_contract: ExecutionContractId::Field,
        }],
    }
}
fn accepted(
    d: SelectionDescriptor,
    blobs: &BTreeMap<ArtifactDigest, Vec<u8>>,
) -> VerifiedSelection {
    verify(d, &borrowed(blobs), SelectionLimits::default()).unwrap()
}
fn rejected(d: SelectionDescriptor, blobs: &BTreeMap<ArtifactDigest, Vec<u8>>) -> Error {
    verify(d, &borrowed(blobs), SelectionLimits::default()).unwrap_err()
}

#[test]
fn independent_selected_closure_omits_unused_code_and_opaque_payloads() {
    let s = source(|_| {});
    let compiled = s.compile().unwrap();
    assert_eq!(compiled.verified().payloads().len(), 4);
    assert_eq!(compiled.verified().artifacts().len(), 7); // 2 roots + 4 payloads + 1 code
    for unused in [
        b"euler-kernel-fixture".as_slice(),
        b"unselected-icon",
        b"not even valid CBOR",
    ] {
        assert!(
            !compiled
                .verified()
                .artifacts()
                .contains_key(&ArtifactDigest::sha256_of(unused))
        );
    }
    let (d, exported) = s.exported();
    let verified = accepted(d.clone(), &exported); // no installation or inventory
    assert_eq!(verified.descriptor(), &d);
    assert_eq!(verified.artifacts(), compiled.verified().artifacts());
    // Irrelevant corrupt cached data never acquires dependency/activation status.
    let mut noisy = exported;
    noisy.insert(ArtifactDigest::sha256_of(b"unselected-icon"), vec![255]);
    assert_eq!(accepted(d, &noisy).artifacts(), verified.artifacts());
}

#[test]
fn deterministic_compilation_and_bounded_canonical_roundtrip() {
    let mut s = source(|_| {});
    let first = s.compile().unwrap();
    s.releases.reverse();
    let second = s.compile().unwrap();
    assert_eq!(first.descriptor_bytes(), second.descriptor_bytes());
    let limits = SelectionLimits::default();
    let d = SelectionDescriptor::from_cbor(first.descriptor_bytes(), limits).unwrap();
    assert_eq!(d.to_cbor(limits).unwrap(), first.descriptor_bytes());
    for end in 0..first.descriptor_bytes().len() {
        assert!(SelectionDescriptor::from_cbor(&first.descriptor_bytes()[..end], limits).is_err());
    }
    let mut trailing = first.descriptor_bytes().to_vec();
    trailing.push(0);
    assert!(SelectionDescriptor::from_cbor(&trailing, limits).is_err());
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(
        serde_json::from_str::<SelectionDescriptor>(&json).unwrap(),
        d
    );
    assert!(
        serde_json::from_str::<SelectionDescriptor>(&json.replacen("{", "{\"unknown\":1,", 1))
            .is_err()
    );
    assert_eq!(
        SelectionDescriptor::from_cbor(
            first.descriptor_bytes(),
            SelectionLimits {
                descriptor_bytes: first.descriptor_bytes().len() - 1,
                ..limits
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
}

#[test]
fn required_blob_missing_corrupt_or_wrong_length_is_rejected() {
    let (d, blobs) = source(|_| {}).exported();
    for digest in blobs.keys() {
        let mut missing = blobs.clone();
        missing.remove(digest);
        assert_eq!(
            rejected(d.clone(), &missing).code,
            ErrorCode::InvalidSelection
        );
        let mut corrupt = blobs.clone();
        corrupt.get_mut(digest).unwrap()[0] ^= 1;
        assert_eq!(
            rejected(d.clone(), &corrupt).code,
            ErrorCode::IntegrityMismatch
        );
        let mut truncated = blobs.clone();
        truncated.get_mut(digest).unwrap().pop();
        assert_eq!(
            rejected(d.clone(), &truncated).code,
            ErrorCode::IntegrityMismatch
        );
    }
}

#[test]
fn exact_edges_and_canonical_membership_are_not_trusted() {
    let (d, blobs) = source(|_| {}).exported();
    let mut missing = d.clone();
    missing.bindings.pop();
    assert_eq!(rejected(missing, &blobs).path, "bindings");
    let mut wrong = d.clone();
    wrong.bindings[0].exact_contract = wrong.bindings[1].exact_contract.clone();
    // Ensure different requirement contracts regardless of release hash order.
    wrong.bindings[0].exact_contract.name = "org.example.wrong".parse().unwrap();
    assert_eq!(rejected(wrong, &blobs).path, "bindings");
    let mut extra = d.clone();
    let mut edge = extra.bindings[0].clone();
    edge.requirement_slot = "unexpected".parse().unwrap();
    extra.bindings.push(edge);
    extra.bindings.sort_by(|a, b| {
        (&a.consumer, &a.requirement_slot).cmp(&(&b.consumer, &b.requirement_slot))
    });
    assert_eq!(rejected(extra, &blobs).path, "bindings");
    let mut duplicate = d.clone();
    duplicate
        .contributions
        .push(duplicate.contributions[0].clone());
    assert!(rejected(duplicate, &blobs).message.contains("order"));
    let mut reverse = d.clone();
    reverse.bindings.reverse();
    assert!(rejected(reverse, &blobs).message.contains("order"));
    let mut member = d.clone();
    member.contributions[0].local_id = "absent".parse().unwrap();
    member.contributions.sort();
    assert!(verify(member, &borrowed(&blobs), Default::default()).is_err());
    let mut evidence = d.clone();
    evidence.release_evidence.pop();
    assert_eq!(rejected(evidence, &blobs).path, "releaseEvidence");
    let mut version = d.clone();
    version.api_version = "orishu.plugin-selection/v2".parse().unwrap();
    assert_eq!(
        rejected(version, &blobs).code,
        ErrorCode::UnsupportedVersion
    );
    let mut roots = d;
    roots.roots.clear();
    assert!(rejected(roots, &blobs).message.contains("unreachable"));
}

#[test]
fn selected_executables_require_exact_configured_roles() {
    let (d, blobs) = source(|_| {}).exported();
    let mut missing = d.clone();
    missing.kernel_instances.clear();
    assert_eq!(rejected(missing, &blobs).path, "kernelInstances");
    let mut wrong = d.clone();
    wrong.kernel_instances[0].execution_contract = ExecutionContractId::Dynamics;
    assert_eq!(rejected(wrong, &blobs).path, "kernelInstances");
    let mut vocabulary = d.clone();
    vocabulary.kernel_instances[0].contribution = vocabulary
        .contributions
        .iter()
        .find(|c| c.local_id.as_str() == "mass")
        .unwrap()
        .clone();
    assert_eq!(rejected(vocabulary, &blobs).path, "kernelInstances");
    let mut duplicate = d;
    duplicate
        .kernel_instances
        .push(duplicate.kernel_instances[0].clone());
    assert!(rejected(duplicate, &blobs).message.contains("order"));
}

#[test]
fn local_dependencies_cannot_rebind_to_semantically_identical_external_provider() {
    let s = source(|_| {});
    let (mut d, mut blobs) = s.exported();
    let declarations = declarations();
    let (root, additional) = release("org.example.other", &declarations[2..3], &[]);
    let release =
        VerifiedRelease::verify(root, &borrowed(&additional), &Limits::default()).unwrap();
    let alternative = release
        .contribution_ref(&"acceleration".parse().unwrap())
        .unwrap();
    let root_bytes = release.root().canonical_bytes(&Limits::default()).unwrap();
    let artifact = Artifact {
        digest: ArtifactDigest::sha256_of(&root_bytes),
        size_bytes: root_bytes.len() as u64,
        media_type: EVIDENCE_MEDIA_TYPE.into(),
    };
    blobs.insert(artifact.digest, root_bytes);
    blobs.extend(additional);
    d.release_evidence.push(ReleaseEvidence {
        release: release.id(),
        artifact,
    });
    d.release_evidence.sort_by_key(|e| e.release);
    d.contributions.push(alternative.clone());
    d.contributions.sort();
    d.roots.push(alternative.clone());
    d.roots.sort();
    d.bindings
        .iter_mut()
        .find(|b| b.consumer.local_id.as_str() == "gravity")
        .unwrap()
        .provider = alternative.clone();
    assert!(
        rejected(d.clone(), &blobs)
            .message
            .contains("local dependency")
    );
    // An external requirement *can* explicitly select that exact provider.
    d.bindings
        .iter_mut()
        .find(|b| b.consumer.local_id.as_str() == "gravity")
        .unwrap()
        .provider = s.releases[0]
        .contribution_ref(&"acceleration".parse().unwrap())
        .unwrap();
    d.bindings
        .iter_mut()
        .find(|b| {
            b.consumer.local_id.as_str() == "classical"
                && b.requirement_slot.as_str() == "acceleration"
        })
        .unwrap()
        .provider = alternative;
    accepted(d, &blobs);
}

#[test]
fn exact_semantics_do_not_substitute_for_dependency_role_checks() {
    let s = source(|d| {
        let dynamics = d[1].1.contract_ref(&Limits::default()).unwrap();
        let Payload::FieldModels(model) = &mut d[5].1 else {
            unreachable!()
        };
        model
            .scientific
            .requirements
            .iter_mut()
            .find(|r| r.slot.as_str() == "mass")
            .unwrap()
            .contract = dynamics;
    });
    // Resolver can supply the exact requested contract; it has the wrong role
    // for a field-model coupling and must not become a runnable projection.
    assert!(s.compile().unwrap_err().message.contains("role"));
    let s = source(|d| {
        let Payload::FieldModels(model) = &mut d[5].1 else {
            unreachable!()
        };
        model.scientific.observables.clear();
    });
    assert!(s.compile().unwrap_err().message.contains("channels"));
}

#[test]
fn selected_unknown_point_fails_closed_but_unselected_is_opaque() {
    let s = source(|_| {});
    let (mut d, mut blobs) = s.exported();
    let unknown = s.releases[0]
        .contribution_ref(&"future".parse().unwrap())
        .unwrap();
    d.contributions.push(unknown.clone());
    d.contributions.sort();
    d.roots.push(unknown);
    d.roots.sort();
    // No need to deserialize this opaque data even to reject its active use.
    blobs.insert(
        ArtifactDigest::sha256_of(b"not even valid CBOR"),
        b"not even valid CBOR".to_vec(),
    );
    assert_eq!(rejected(d, &blobs).code, ErrorCode::UnsupportedVersion);
}

#[test]
fn per_collection_and_aggregate_limits_apply_to_raw_and_wire_input() {
    let (d, blobs) = source(|_| {}).exported();
    let verified = accepted(d.clone(), &blobs);
    let limits = SelectionLimits::default();
    let wire = d.to_cbor(limits).unwrap();
    let total: u64 = verified.artifacts().values().map(|a| a.size_bytes).sum();
    for l in [
        SelectionLimits {
            contributions: d.contributions.len() - 1,
            ..limits
        },
        SelectionLimits {
            bindings: d.bindings.len() - 1,
            ..limits
        },
        SelectionLimits {
            kernel_instances: 0,
            ..limits
        },
        SelectionLimits {
            releases: 1,
            ..limits
        },
        SelectionLimits {
            artifacts: verified.artifacts().len() - 1,
            ..limits
        },
        SelectionLimits {
            artifact_bytes: total - 1,
            ..limits
        },
        SelectionLimits {
            metadata_bytes: 0,
            ..limits
        },
        SelectionLimits {
            descriptor_bytes: wire.len() - 1,
            ..limits
        },
    ] {
        assert_eq!(
            verify(d.clone(), &borrowed(&blobs), l).unwrap_err().code,
            ErrorCode::LimitExceeded
        );
    }
    assert!(
        verify(
            d,
            &borrowed(&blobs),
            SelectionLimits {
                artifact_bytes: total,
                ..limits
            }
        )
        .is_ok()
    );
}

#[test]
fn offline_declarations_never_satisfy_executable_closure() {
    let (descriptor, blobs) = source(|_| {}).exported();
    let full = accepted(descriptor.clone(), &blobs);
    let metadata = full.declarations().blobs(&Limits::default()).unwrap();
    assert!(metadata.len() < blobs.len());
    assert!(!metadata.contains_key(&ArtifactDigest::sha256_of(b"classical-kernel-fixture")));
    let offline = verify_declarations(
        descriptor.clone(),
        &borrowed(&metadata),
        SelectionLimits::default(),
    )
    .unwrap();
    assert_eq!(offline, full.declarations());
    assert!(
        verify(
            descriptor.clone(),
            &borrowed(&metadata),
            SelectionLimits::default()
        )
        .is_err()
    );
    let mut corrupt = metadata.clone();
    corrupt.values_mut().next().unwrap()[0] ^= 1;
    assert!(
        verify_declarations(
            descriptor.clone(),
            &borrowed(&corrupt),
            SelectionLimits::default()
        )
        .is_err()
    );
    for id in metadata.keys() {
        let mut missing = metadata.clone();
        missing.remove(id);
        assert!(
            verify_declarations(
                descriptor.clone(),
                &borrowed(&missing),
                SelectionLimits::default()
            )
            .is_err()
        );
    }
    assert_eq!(
        verify_declarations(
            descriptor,
            &borrowed(&metadata),
            SelectionLimits {
                metadata_bytes: 1,
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
}

#[test]
fn compiler_rechecks_selection_and_source_bytes_instead_of_trusting_handles() {
    let mut s = source(|_| {});
    s.blobs
        .get_mut(&ArtifactDigest::sha256_of(b"classical-kernel-fixture"))
        .unwrap()[0] ^= 1;
    assert_eq!(s.compile().unwrap_err().code, ErrorCode::IntegrityMismatch);
    let mut s = source(|_| {});
    s.selection.bindings.pop();
    assert_eq!(s.compile().unwrap_err().path, "bindings");
    let mut s = source(|_| {});
    s.releases.pop();
    assert_eq!(s.compile().unwrap_err().path, "releaseEvidence");
}

#[test]
fn one_selected_artifact_cannot_claim_two_execution_contracts_across_releases() {
    let mut s = source(|d| {
        let Payload::Integrators(v) = &mut d[6].1 else {
            unreachable!()
        };
        v.scientific.kernel = ArtifactDigest::sha256_of(b"classical-kernel-fixture");
    });
    let integrator = s.releases[0]
        .contribution_ref(&"euler".parse().unwrap())
        .unwrap();
    let entries: Vec<_> = s
        .releases
        .iter()
        .map(|release| InventoryEntry {
            release,
            enabled: true,
            is_default: true,
        })
        .collect();
    let inventory = Inventory::new(1, &entries, Default::default()).unwrap();
    let mut roots = s.selection.roots.clone();
    roots.push(integrator.clone());
    roots.sort();
    let ResolutionOutcome::Resolved { selection, .. } = inventory
        .resolve(&ResolutionRequest {
            expected_inventory_revision: 1,
            roots,
            bindings: vec![],
        })
        .unwrap()
    else {
        panic!("fixture must resolve")
    };
    s.selection = selection;
    s.instances.push(SelectedKernel {
        instance_id: "dynamics-1".parse().unwrap(),
        contribution: integrator,
        execution_contract: ExecutionContractId::Dynamics,
    });
    s.instances
        .sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
    assert!(
        s.compile()
            .unwrap_err()
            .message
            .contains("two execution contracts")
    );
}

fn field_context(selected: &VerifiedSelection) -> execution::InstanceContext {
    use execution::*;
    let instance = &selected.descriptor().kernel_instances[0];
    let model = &selected.payloads()[&instance.contribution];
    let Payload::FieldModels(field) = model else {
        unreachable!()
    };
    let mass = selected
        .payloads()
        .iter()
        .find(|(c, _)| c.local_id.as_str() == "mass")
        .unwrap()
        .1;
    let channel = selected
        .payloads()
        .iter()
        .find(|(c, _)| c.local_id.as_str() == "acceleration")
        .unwrap()
        .1;
    let Payload::Observables(observable) = channel else {
        unreachable!()
    };
    let property = CouplingProperty {
        property: "mass".parse().unwrap(),
        dimension: Dimension::MASS,
    };
    InstanceContext {
        api_version: INSTANCE_SCHEMA.parse().unwrap(),
        instance: instance.instance_id.clone(),
        kernel: field.scientific.kernel,
        contribution: instance.contribution.clone(),
        scientific: model.contract_ref(&Limits::default()).unwrap(),
        execution_contract: ExecutionContractId::Field,
        state_format: field.scientific.state_format.clone(),
        profile: ExecutionProfile::ForceThenIntegrate,
        configuration: InputIdentity::of(CONFIGURATION_SCHEMA.parse().unwrap(), 1, b"config"),
        domain: InputIdentity::of(DOMAIN_SCHEMA.parse().unwrap(), 1, b"domain"),
        couplings: vec![CouplingDescriptor {
            slot: "mass".parse().unwrap(),
            component: mass.contract_ref(&Limits::default()).unwrap(),
            source: Some(property.clone()),
            response: Some(property),
        }],
        observables: vec![ObservableBinding {
            slot: "acceleration".parse().unwrap(),
            quality_flags: 1,
            channel: SampleChannel {
                contract: channel.contract_ref(&Limits::default()).unwrap(),
                schema: observable.scientific.clone(),
            },
        }],
        compute_precision: ComputePrecision::Binary64,
        bounds: ExecutionBounds {
            projection_records: 10,
            state_bytes: 1024,
            sample_points: 10,
            sample_channels: 1,
        },
    }
}

#[test]
fn captured_context_cannot_change_selected_scientific_semantics() {
    let compiled = source(|_| {}).compile().unwrap();
    let selected = compiled.verified();
    let c = field_context(selected);
    verify_context(selected, &c, &Limits::default()).unwrap();
    for case in 0..11 {
        let mut c = c.clone();
        match case {
            0 => c.instance = "absent".parse().unwrap(),
            1 => c.kernel = ArtifactDigest::sha256_of(b"other code"),
            2 => c.scientific.name = "org.example.other".parse().unwrap(),
            3 => c.state_format.version = 2.try_into().unwrap(),
            4 => c.bounds.state_bytes += 1,
            5 => c.couplings[0].source.as_mut().unwrap().dimension = Dimension::LENGTH,
            6 => c.couplings[0].response = None,
            7 => c.couplings[0].component.name = "org.example.other".parse().unwrap(),
            8 => c.observables.clear(),
            9 => c.observables[0].slot = "other".parse().unwrap(),
            _ => c.contribution.local_id = "mass".parse().unwrap(),
        }
        assert!(
            verify_context(selected, &c, &Limits::default()).is_err(),
            "case {case}"
        );
    }
    let provenance = selected.releases().get(&c.contribution.release).unwrap();
    assert_eq!(provenance.plugin_id.as_str(), "org.example.solver");
}

#[test]
fn context_builder_uses_exact_vocabulary_and_requires_explicit_host_choices() {
    let compiled = source(|_| {}).compile().unwrap();
    let selected = compiled.verified();
    let expected = field_context(selected);
    let inputs = ContextInputs {
        configuration: expected.configuration.clone(),
        domain: expected.domain.clone(),
        compute_precision: expected.compute_precision,
        bounds: expected.bounds,
        quality_flags: [("acceleration".parse().unwrap(), 1)].into(),
    };
    assert_eq!(
        build_context(
            selected,
            &expected.instance,
            inputs.clone(),
            &Limits::default()
        )
        .unwrap(),
        expected
    );
    assert!(
        build_context(
            selected,
            &"missing".parse().unwrap(),
            inputs.clone(),
            &Limits::default()
        )
        .is_err()
    );
    for case in 0..5 {
        let mut bad = inputs.clone();
        match case {
            0 => bad.quality_flags.clear(),
            1 => {
                bad.quality_flags.insert("undeclared".parse().unwrap(), 1);
            }
            2 => {
                *bad.quality_flags.values_mut().next().unwrap() = 0;
            }
            3 => {
                *bad.quality_flags.values_mut().next().unwrap() = 8;
            }
            _ => bad.bounds.state_bytes += 1,
        }
        assert!(
            build_context(selected, &expected.instance, bad, &Limits::default()).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn captured_execution_is_canonical_and_never_initializes_or_guesses_state() {
    use execution::*;
    let compiled = source(|_| {}).compile().unwrap();
    let c = field_context(compiled.verified());
    let kernel = CapturedKernel {
        instance: c.instance.clone(),
        context: InputIdentity::of(INSTANCE_SCHEMA.parse().unwrap(), 1, &c.to_cbor().unwrap()),
        state: InputIdentity::of(
            "org.example.classical.state/v1".parse().unwrap(),
            0,
            b"captured",
        ),
    };
    let mut dynamics = kernel.clone();
    dynamics.instance = "dynamics".parse().unwrap();
    let d = ExecutionDefinition {
        scene: None,
        api_version: EXECUTION_SCHEMA.parse().unwrap(),
        profile: ExecutionProfile::ForceThenIntegrate,
        timestep_seconds: FiniteF64::new(0.25).unwrap(),
        objects: InputIdentity::of(ObjectState::SCHEMA.parse().unwrap(), 0, b"objects"),
        fields: vec![CapturedField {
            kernel,
            coupled: InputIdentity::of(CoupledEntity::SCHEMA.parse().unwrap(), 0, b"coupled"),
        }],
        dynamics,
    };
    let bytes = d.to_cbor(64).unwrap();
    let mut scene_bearing = d.clone();
    scene_bearing.api_version = EXECUTION_SCENE_SCHEMA.parse().unwrap();
    assert!(scene_bearing.to_cbor(64).is_err(), "v2 requires a scene");
    scene_bearing.scene = Some(InputIdentity::of(
        SCENE_SCHEMA.parse().unwrap(),
        1,
        b"scene",
    ));
    let v2 = scene_bearing.to_cbor(64).unwrap();
    assert_eq!(
        ExecutionDefinition::from_cbor(&v2, 64).unwrap(),
        scene_bearing
    );
    scene_bearing.api_version = EXECUTION_SCHEMA.parse().unwrap();
    assert!(
        scene_bearing.to_cbor(64).is_err(),
        "v1 must not acquire new scene semantics"
    );
    assert_eq!(ExecutionDefinition::from_cbor(&bytes, 64).unwrap(), d);
    for end in 0..bytes.len() {
        assert!(ExecutionDefinition::from_cbor(&bytes[..end], 64).is_err());
    }
    assert!(ExecutionDefinition::from_cbor(&bytes, 0).is_err());
    for case in 0..6 {
        let mut d = d.clone();
        match case {
            0 => d.timestep_seconds = FiniteF64::ZERO,
            1 => d.fields.push(d.fields[0].clone()),
            2 => d.dynamics.instance = d.fields[0].kernel.instance.clone(),
            3 => d.fields[0].kernel.context.value_count = 2,
            4 => d.objects.schema = "other/v1".parse().unwrap(),
            _ => d.fields[0].coupled.schema = "other/v1".parse().unwrap(),
        }
        assert!(d.to_cbor(64).is_err(), "case {case}");
    }
    let mut no_fields = d;
    no_fields.fields.clear();
    let bytes = no_fields.to_cbor(0).unwrap();
    assert_eq!(
        ExecutionDefinition::from_cbor(&bytes, 0).unwrap(),
        no_fields
    );
}
