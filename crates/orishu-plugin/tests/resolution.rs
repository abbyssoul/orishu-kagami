mod support;

use orishu_plugin::{resolution::*, *};
use support::*;

fn verified(name: &str, items: &[(&str, Payload)], extra: &[&[u8]]) -> VerifiedRelease {
    let (root, blobs) = release(name, items, extra);
    VerifiedRelease::verify(root, &borrowed(&blobs), &Limits::default()).unwrap()
}
fn entry(release: &VerifiedRelease) -> InventoryEntry<'_> {
    InventoryEntry {
        release,
        enabled: true,
        is_default: true,
    }
}
fn request(r: &VerifiedRelease, local: &str) -> ResolutionRequest {
    ResolutionRequest {
        expected_inventory_revision: 1,
        roots: vec![r.contribution_ref(&local.parse().unwrap()).unwrap()],
        bindings: vec![],
    }
}
fn selection(result: ResolutionOutcome) -> Selection {
    match result {
        ResolutionOutcome::Resolved { selection, .. } => selection,
        other => panic!("expected resolution: {other:?}"),
    }
}
fn issues(result: ResolutionOutcome) -> Vec<ResolutionIssue> {
    match result {
        ResolutionOutcome::Unavailable { issues, .. } => issues,
        other => panic!("expected unavailable: {other:?}"),
    }
}

#[test]
fn independent_solver_is_dormant_until_vocabulary_is_available() {
    let d = declarations();
    let a = verified("org.example.vocabulary", &d[..5], &[]);
    let b = verified(
        "org.example.solver",
        &d[5..6],
        &[b"classical-kernel-fixture"],
    );
    let req = request(&b, "classical");
    let solo = Inventory::new(1, &[entry(&b)], Default::default()).unwrap();
    let pending = issues(solo.resolve(&req).unwrap());
    assert_eq!(pending.len(), 3);
    assert!(
        pending
            .iter()
            .all(|i| i.reason == UnavailableReason::MissingProvider)
    );
    let complete = Inventory::new(1, &[entry(&b), entry(&a)], Default::default()).unwrap();
    let selected = selection(complete.resolve(&req).unwrap());
    assert_eq!(selected.contributions.len(), 4); // solver + gravity + mass + acceleration
    assert_eq!(selected.bindings.len(), 4); // includes gravity's acceleration dependency
    assert!(
        selected
            .contributions
            .iter()
            .all(|c| c.release == a.id() || c.release == b.id())
    );
}

#[test]
fn order_does_not_select_a_provider_and_existing_pins_survive_alternatives() {
    let d = declarations();
    let a = verified("org.example.a", &d[..5], &[]);
    let c = verified("org.example.c", &d[..5], &[]);
    let b = verified(
        "org.example.solver",
        &d[5..6],
        &[b"classical-kernel-fixture"],
    );
    let mut req = request(&b, "classical");
    let initial = Inventory::new(1, &[entry(&a), entry(&b)], Default::default()).unwrap();
    let original = selection(initial.resolve(&req).unwrap());
    let one = Inventory::new(1, &[entry(&a), entry(&b), entry(&c)], Default::default()).unwrap();
    let two = Inventory::new(1, &[entry(&c), entry(&a), entry(&b)], Default::default()).unwrap();
    assert_eq!(one.resolve(&req).unwrap(), two.resolve(&req).unwrap());
    assert!(
        issues(one.resolve(&req).unwrap())
            .iter()
            .all(|i| i.reason == UnavailableReason::AmbiguousProvider && i.candidate_count == 2)
    );
    req.bindings = original.bindings.clone();
    assert_eq!(selection(one.resolve(&req).unwrap()), original);
    assert_eq!(selection(two.resolve(&req).unwrap()), original);
}

#[test]
fn explicit_ambiguity_answers_are_revalidated_not_trusted() {
    let d = declarations();
    let a = verified("org.example.a", &d[..5], &[]);
    let c = verified("org.example.c", &d[..5], &[]);
    let b = verified("org.example.b", &d[5..6], &[b"classical-kernel-fixture"]);
    let inventory =
        Inventory::new(1, &[entry(&a), entry(&b), entry(&c)], Default::default()).unwrap();
    let mut req = request(&b, "classical");
    // Respond to ambiguity until all newly revealed transitive requirements have choices.
    for _ in 0..3 {
        let response = inventory.resolve(&req).unwrap();
        if matches!(response, ResolutionOutcome::Resolved { .. }) {
            break;
        }
        for issue in issues(response) {
            assert_eq!(issue.reason, UnavailableReason::AmbiguousProvider);
            req.bindings.push(ProviderBinding {
                requirement: issue.requirement.unwrap(),
                provider: issue
                    .candidates
                    .into_iter()
                    .find(|p| p.release == a.id())
                    .unwrap(),
            });
        }
    }
    assert_eq!(
        selection(inventory.resolve(&req).unwrap()).bindings.len(),
        4
    );
    let newer = Inventory::new(2, &[entry(&a), entry(&b), entry(&c)], Default::default()).unwrap();
    assert_eq!(
        newer.resolve(&req).unwrap(),
        ResolutionOutcome::StaleRevision {
            expected: 1,
            actual: 2
        }
    );
    req.bindings[0].provider = req.roots[0].clone();
    assert!(
        issues(inventory.resolve(&req).unwrap())
            .iter()
            .any(|i| i.reason == UnavailableReason::IncompatiblePin)
    );
}

#[test]
fn nondefault_release_is_only_selected_explicitly_and_missing_pins_never_substitute() {
    let d = declarations();
    let old = verified("org.example.a", &d[..5], &[]);
    let (mut root, blobs) = release("org.example.a", &d[..5], &[]);
    root.0.metadata.version_label = "v2".into();
    let new = VerifiedRelease::verify(root, &borrowed(&blobs), &Limits::default()).unwrap();
    let b = verified("org.example.b", &d[5..6], &[b"classical-kernel-fixture"]);
    let initial = Inventory::new(1, &[entry(&old), entry(&b)], Default::default()).unwrap();
    let mut req = request(&b, "classical");
    let original = selection(initial.resolve(&req).unwrap());
    let inventory = Inventory::new(
        1,
        &[
            InventoryEntry {
                is_default: false,
                ..entry(&old)
            },
            entry(&new),
            entry(&b),
        ],
        Default::default(),
    )
    .unwrap();
    let unbound = selection(inventory.resolve(&req).unwrap());
    assert!(
        unbound
            .bindings
            .iter()
            .all(|p| p.provider.release == new.id())
    );
    req.bindings = original.bindings;
    let pinned = selection(inventory.resolve(&req).unwrap());
    assert!(
        pinned
            .bindings
            .iter()
            .all(|p| p.provider.release == old.id())
    );
    let removed = Inventory::new(1, &[entry(&new), entry(&b)], Default::default()).unwrap();
    assert!(
        issues(removed.resolve(&req).unwrap())
            .iter()
            .any(|i| i.reason == UnavailableReason::MissingRelease)
    );
}

#[test]
fn disabled_pins_and_same_name_different_digest_are_not_replacements() {
    let d = declarations();
    let a = verified("org.example.a", &d[..5], &[]);
    let b = verified("org.example.b", &d[5..6], &[b"classical-kernel-fixture"]);
    let initial = Inventory::new(1, &[entry(&a), entry(&b)], Default::default()).unwrap();
    let mut req = request(&b, "classical");
    req.bindings = selection(initial.resolve(&req).unwrap()).bindings;
    let disabled = Inventory::new(
        1,
        &[
            InventoryEntry {
                enabled: false,
                ..entry(&a)
            },
            entry(&b),
        ],
        Default::default(),
    )
    .unwrap();
    assert!(
        issues(disabled.resolve(&req).unwrap())
            .iter()
            .any(|i| i.reason == UnavailableReason::Disabled)
    );
    let mut changed = d[..5].to_vec();
    if let Payload::Components(p) = &mut changed[0].1 {
        p.presentation = Some(Annotations {
            label: Some("Presentation doesn't alter contract".into()),
            ..Default::default()
        });
        p.scientific.properties[0].required = true;
        if let PropertyType::Quantity {
            default_expression, ..
        } = &mut p.scientific.properties[0].schema
        {
            *default_expression = Some("2 kg".into());
        }
    }
    let changed = verified("org.example.changed", &changed, &[]);
    let inventory = Inventory::new(1, &[entry(&changed), entry(&b)], Default::default()).unwrap();
    req.bindings.clear();
    assert!(
        issues(inventory.resolve(&req).unwrap())
            .iter()
            .any(|i| i.reason == UnavailableReason::MissingProvider
                && i.requirement.as_ref().unwrap().slot.as_str() == "mass")
    );
}

#[test]
fn opaque_local_dependency_does_not_disable_independent_contributions() {
    let d = declarations();
    let (mut root, mut blobs) = release(
        "org.example.a",
        &d,
        &[b"classical-kernel-fixture", b"euler-kernel-fixture"],
    );
    let bytes = b"future opaque vocabulary";
    let digest = ArtifactDigest::sha256_of(bytes);
    blobs.insert(digest, bytes.to_vec());
    root.0.spec.artifacts.push(Artifact {
        digest,
        size_bytes: bytes.len() as u64,
        media_type: "application/octet-stream".into(),
    });
    root.0.spec.contributions.push(Contribution {
        local_id: "future".parse().unwrap(),
        extension_point: "org.example.future/v1".parse().unwrap(),
        payload: digest,
        requirements: vec![],
        annotations: None,
    });
    let solver = root
        .0
        .spec
        .contributions
        .iter_mut()
        .find(|c| c.local_id.as_str() == "classical")
        .unwrap();
    let mass = solver
        .requirements
        .iter_mut()
        .find(|r| r.slot().as_str() == "mass")
        .unwrap();
    *mass = Requirement::Local {
        slot: "mass".parse().unwrap(),
        local_contribution: "future".parse().unwrap(),
    };
    let a = VerifiedRelease::verify(root, &borrowed(&blobs), &Limits::default()).unwrap();
    let inventory = Inventory::new(1, &[entry(&a)], Default::default()).unwrap();
    assert!(
        issues(inventory.resolve(&request(&a, "classical")).unwrap())
            .iter()
            .any(|i| i.reason == UnavailableReason::UnsupportedContribution)
    );
    assert_eq!(
        selection(inventory.resolve(&request(&a, "mass")).unwrap())
            .contributions
            .len(),
        1
    );
}

#[test]
fn snapshot_invariants_and_request_counts_are_checked() {
    let d = declarations();
    let a = verified("org.example.a", &d[..5], &[]);
    assert!(Inventory::new(1, &[entry(&a), entry(&a)], Default::default()).is_err());
    assert!(
        Inventory::new(
            1,
            &[InventoryEntry {
                is_default: false,
                ..entry(&a)
            }],
            Default::default()
        )
        .is_err()
    );
    assert!(
        Inventory::new(
            1,
            &[entry(&a)],
            ResolutionLimits {
                max_contributions: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    let inventory = Inventory::new(1, &[entry(&a)], Default::default()).unwrap();
    let mut req = request(&a, "mass");
    req.roots.push(req.roots[0].clone());
    assert!(inventory.resolve(&req).is_err());
    req.roots.pop();
    req.bindings.push(ProviderBinding {
        requirement: RequirementKey {
            consumer: req.roots[0].clone(),
            slot: "bogus".parse().unwrap(),
        },
        provider: req.roots[0].clone(),
    });
    assert_eq!(
        issues(inventory.resolve(&req).unwrap())[0].reason,
        UnavailableReason::UnusedBinding
    );
}

#[test]
fn diagnostics_candidates_depth_and_work_are_bounded() {
    let d = declarations();
    let a = verified("org.example.a", &d[..5], &[]);
    let c = verified("org.example.c", &d[..5], &[]);
    let b = verified("org.example.b", &d[5..6], &[b"classical-kernel-fixture"]);
    let limits = ResolutionLimits {
        max_diagnostics: 1,
        max_candidates: 1,
        ..Default::default()
    };
    let inventory = Inventory::new(1, &[entry(&a), entry(&b), entry(&c)], limits).unwrap();
    let result = inventory.resolve(&request(&b, "classical")).unwrap();
    assert!(matches!(
        result,
        ResolutionOutcome::Unavailable {
            truncated: true,
            ..
        }
    ));
    let details = issues(result);
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].candidate_count, 2);
    assert_eq!(details[0].candidates.len(), 1);
    for limits in [
        ResolutionLimits {
            max_work: 1,
            ..Default::default()
        },
        ResolutionLimits {
            max_dependency_depth: 1,
            ..Default::default()
        },
        ResolutionLimits {
            max_bindings: 1,
            ..Default::default()
        },
    ] {
        let inventory = Inventory::new(1, &[entry(&a), entry(&b)], limits).unwrap();
        assert!(
            issues(inventory.resolve(&request(&b, "classical")).unwrap())
                .iter()
                .any(|i| i.reason == UnavailableReason::LimitExceeded)
        );
    }
    let inventory = Inventory::new(
        1,
        &[entry(&b)],
        ResolutionLimits {
            max_diagnostics: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        matches!(inventory.resolve(&request(&b, "classical")).unwrap(), ResolutionOutcome::Unavailable { issues, truncated: true, .. } if issues.is_empty())
    );
}

#[test]
fn serialized_requests_and_results_preserve_pins_and_revision() {
    let d = declarations();
    let a = verified("org.example.a", &d[..5], &[]);
    let inventory = Inventory::new(1, &[entry(&a)], Default::default()).unwrap();
    let req = request(&a, "gravity");
    let decoded: ResolutionRequest =
        serde_json::from_slice(&serde_json::to_vec(&req).unwrap()).unwrap();
    let response = inventory.resolve(&decoded).unwrap();
    let decoded: ResolutionOutcome =
        serde_json::from_slice(&serde_json::to_vec(&response).unwrap()).unwrap();
    assert_eq!(response, decoded);
    let mut req = req;
    req.bindings = selection(decoded).bindings;
    assert_eq!(inventory.resolve(&req).unwrap(), response);
}

#[test]
fn candidate_pages_cover_every_choice_without_changing_eligibility() {
    let d = declarations();
    let a = verified("org.example.a", &d[..5], &[]);
    let c = verified("org.example.c", &d[..5], &[]);
    let b = verified("org.example.b", &d[5..6], &[b"classical-kernel-fixture"]);
    let limits = ResolutionLimits {
        max_candidates: 1,
        ..Default::default()
    };
    let inventory = Inventory::new(1, &[entry(&a), entry(&b), entry(&c)], limits).unwrap();
    let req = request(&b, "classical");
    let key = RequirementKey {
        consumer: req.roots[0].clone(),
        slot: "mass".parse().unwrap(),
    };
    let CandidatePageResponse::Page {
        candidates: first,
        total: 2,
        next_offset: Some(1),
        ..
    } = inventory.candidate_page(&req, &key, 0).unwrap()
    else {
        panic!("expected first page")
    };
    let CandidatePageResponse::Page {
        candidates: second,
        total: 2,
        next_offset: None,
        ..
    } = inventory.candidate_page(&req, &key, 1).unwrap()
    else {
        panic!("expected second page")
    };
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert!(first < second);
    assert!(inventory.candidate_page(&req, &key, 3).is_err());
    let newer = Inventory::new(2, &[entry(&a), entry(&b), entry(&c)], limits).unwrap();
    assert_eq!(
        newer.candidate_page(&req, &key, 1).unwrap(),
        CandidatePageResponse::StaleRevision {
            expected: 1,
            actual: 2
        }
    );
}
