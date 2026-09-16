#![cfg(unix)]
//! Installed bundles -> exact retained selection -> real sandbox initialization.
use kagami::{plugins::*, scientific::*};
use orishu_plugin::{bundle, execution::*, resolution::*, selected::*, *};
use orishu_runtime::{
    Buffer, Interruption, KernelRejection, OperationControl, Sandbox, SandboxLimits,
};
use orishu_variables::VariablesSystem;
use std::{sync::Arc, time::Duration};

#[path = "../../../plugins/reference/declarations.rs"]
mod declarations;
const GRAVITY: &[u8] =
    include_bytes!("../../../crates/orishu-runtime/tests/fixtures/newtonian.component.wasm");
const EULER: &[u8] =
    include_bytes!("../../../crates/orishu-runtime/tests/fixtures/euler.component.wasm");
const UNUSED: &[u8] = b"unused kernel must not be exported with vocabulary";

fn package(name: &str, payloads: Vec<(LocalContributionId, Payload)>, code: &[&[u8]]) -> Package {
    let (root, blobs) = declarations::release(name, payloads, code).unwrap();
    let borrowed = blobs.iter().map(|(d, b)| (*d, b.as_slice())).collect();
    let bytes = bundle::pack(&root, &borrowed, &Limits::default(), Default::default()).unwrap();
    Package::from_bundle(&bytes).unwrap()
}
struct Installed {
    store: PluginStore,
    request: ResolutionRequest,
    uses: Vec<SelectedKernel>,
    vocabulary: PluginReleaseId,
    _dir: tempfile::TempDir,
}
impl Installed {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = PluginStore::open(dir.path()).unwrap();
        let vocabulary = package(
            "org.orishu.reference.vocabulary",
            declarations::vocabulary(),
            &[UNUSED],
        );
        let models = package(
            "org.orishu.reference.solvers",
            vec![
                (
                    "newtonian".parse().unwrap(),
                    declarations::newtonian(ArtifactDigest::sha256_of(GRAVITY)),
                ),
                (
                    "euler".parse().unwrap(),
                    declarations::euler(ArtifactDigest::sha256_of(EULER)),
                ),
            ],
            &[GRAVITY, EULER],
        );
        let revision = store.install(0, &vocabulary, None, None).unwrap();
        let revision = store.install(revision, &models, None, None).unwrap();
        let uses: Vec<_> = [
            ("euler", ExecutionContractId::Dynamics),
            ("newtonian", ExecutionContractId::Field),
        ]
        .map(|(name, execution_contract)| SelectedKernel {
            instance_id: name.parse().unwrap(),
            contribution: models
                .release()
                .contribution_ref(&name.parse().unwrap())
                .unwrap(),
            execution_contract,
        })
        .into();
        let mut roots: Vec<_> = uses.iter().map(|k| k.contribution.clone()).collect();
        roots.sort();
        Self {
            store,
            request: ResolutionRequest {
                expected_inventory_revision: revision,
                roots,
                bindings: vec![],
            },
            uses,
            vocabulary: vocabulary.release().id(),
            _dir: dir,
        }
    }
    fn prepare(&self) -> PreparedSelection {
        let PrepareSelectionOutcome::Ready(result) = self
            .store
            .prepare_selection(&self.request, &[], &self.uses, Default::default())
            .unwrap()
        else {
            panic!("reference contributions resolve")
        };
        *result
    }
}
fn request(name: &str) -> InitializationRequest {
    InitializationRequest {
        instance: name.parse().unwrap(),
        domain: DomainDescriptor {
            api_version: DOMAIN_SCHEMA.parse().unwrap(),
            lower_metres: [FiniteF64::new(-10.0).unwrap(); 3],
            upper_metres: [FiniteF64::new(10.0).unwrap(); 3],
            discretization: SpatialDiscretization::Continuous,
        },
        configuration: vec![],
        compute_precision: ComputePrecision::Binary64,
        quality_flags: if name == "newtonian" {
            ["acceleration", "jacobian", "potential"]
                .map(|s| (s.parse().unwrap(), 1))
                .into()
        } else {
            Default::default()
        },
    }
}
fn initializer(bytes: usize) -> LocalInitializer {
    LocalInitializer::new(
        Sandbox::new(SandboxLimits::default()).unwrap(),
        InitializationLimits {
            state_bytes: bytes,
            state_values: 65_536,
            ..Default::default()
        },
    )
}
fn control() -> OperationControl {
    OperationControl::new(Duration::from_secs(60)).unwrap()
}
fn entities() -> Buffer {
    let n = |v| FiniteF64::new(v).unwrap();
    let values = [DynamicEntity {
        id: EntityId(7),
        kinematics: Kinematics {
            position_metres: [n(1.0), n(0.0), n(0.0)],
            velocity_metres_per_second: [n(0.0); 3],
        },
        inertial_mass_kilograms: n(2.0),
    }];
    let mut bytes = vec![];
    encode_batch(&values, &mut bytes, BulkLimits::default()).unwrap();
    Buffer {
        schema: DynamicEntity::SCHEMA.into(),
        value_count: 1,
        bytes: bytes.into(),
    }
}

#[test]
fn retained_selection_is_exact_bounded_and_holds_real_removal_leases() {
    let installed = Installed::new();
    let prepared = installed.prepare();
    assert_eq!(
        prepared.inventory_revision(),
        installed.request.expected_inventory_revision
    );
    assert!(
        !prepared
            .blobs()
            .contains_key(&ArtifactDigest::sha256_of(UNUSED))
    );
    assert_eq!(
        prepared.blobs().len(),
        prepared.compiled().verified().artifacts().len()
    );
    let remove = InventoryCommand::Remove {
        plugin_id: "org.orishu.reference.vocabulary".parse().unwrap(),
        release: installed.vocabulary,
        ack_open_references: false,
    };
    assert_eq!(
        installed
            .store
            .submit(prepared.inventory_revision(), remove.clone())
            .unwrap_err()
            .code,
        Code::InUse
    );
    drop(prepared);
    assert!(
        installed
            .store
            .submit(installed.request.expected_inventory_revision, remove)
            .is_ok()
    );
    assert!(matches!(
        installed
            .store
            .prepare_selection(&installed.request, &[], &installed.uses, Default::default())
            .unwrap(),
        PrepareSelectionOutcome::Unresolved(ResolutionOutcome::StaleRevision { .. })
    ));
}

#[test]
fn preparation_refuses_budget_and_disabled_dependency_without_mutation() {
    let installed = Installed::new();
    assert_eq!(
        installed
            .store
            .prepare_selection(
                &installed.request,
                &[],
                &installed.uses,
                SelectionLimits {
                    artifact_bytes: 1,
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
        Code::LimitExceeded
    );
    assert!(matches!(
        installed
            .store
            .prepare_selection(
                &installed.request,
                &[("org.orishu.reference.vocabulary".parse().unwrap(), false)],
                &installed.uses,
                Default::default()
            )
            .unwrap(),
        PrepareSelectionOutcome::Unresolved(_)
    ));
    assert_eq!(
        installed.store.list().unwrap().revision,
        installed.request.expected_inventory_revision
    );
}

#[test]
fn real_initialization_captures_pins_defaults_and_kernel_chosen_bytes() {
    let installed = Installed::new();
    let selected = installed.prepare();
    let vars = VariablesSystem::default();
    let initial = initializer(1024 * 1024);
    let field = initial
        .field(&selected, &request("newtonian"), &vars, control())
        .unwrap();
    assert!(field.history_entities().is_none());
    assert!(field.state().bytes.len() < 1024 * 1024);
    assert!(field.state_identity().matches(
        &field.state().schema,
        field.state().value_count,
        &field.state().bytes
    ));
    assert_eq!(field.context().kernel, ArtifactDigest::sha256_of(GRAVITY));
    assert_eq!(
        field.context().couplings[0]
            .source
            .as_ref()
            .unwrap()
            .property
            .as_str(),
        "source"
    );
    assert_eq!(
        field.context().couplings[0]
            .response
            .as_ref()
            .unwrap()
            .property
            .as_str(),
        "response"
    );
    assert_eq!(field.context().observables.len(), 3);
    let config =
        ResolvedConfiguration::from_cbor(&field.configuration().bytes, &Limits::default()).unwrap();
    assert_eq!(
        config.get("boundary"),
        Some(&ConfigurationValue::Text {
            value: "isolated".into()
        })
    );
    let larger = initializer(2 * 1024 * 1024)
        .field(&selected, &request("newtonian"), &vars, control())
        .unwrap();
    assert_eq!(
        field.state_identity(),
        larger.state_identity(),
        "capacity must not change natural state"
    );
    assert!(Arc::ptr_eq(
        &field.state().bytes,
        &field.clone().state().bytes
    ));
    let dynamics = initial
        .history(&selected, &request("euler"), &vars, entities(), control())
        .unwrap();
    assert_eq!(dynamics.context().kernel, ArtifactDigest::sha256_of(EULER));
    assert_eq!(dynamics.history_entities().unwrap().bytes, entities().bytes);
    assert!(dynamics.context().observables.is_empty());
    assert_eq!(dynamics.context().bounds.sample_points, 0);
}

#[test]
fn typed_failures_leave_prior_captured_state_and_inventory_unchanged() {
    let installed = Installed::new();
    let selected = installed.prepare();
    let initial = initializer(1024 * 1024);
    let vars = VariablesSystem::default();
    let field = initial
        .field(&selected, &request("newtonian"), &vars, control())
        .unwrap();
    let before = field.state_identity().clone();
    let cancelled = control();
    cancelled.cancel();
    assert_eq!(
        initial
            .field(&selected, &request("newtonian"), &vars, cancelled)
            .unwrap_err(),
        InitializationError::Interrupted(Interruption::Cancelled)
    );
    assert_eq!(
        initial
            .field(&selected, &request("euler"), &vars, control())
            .unwrap_err(),
        InitializationError::Selection
    );
    let mut incompatible = request("newtonian");
    incompatible
        .configuration
        .push(AuthoredConfigurationProperty {
            id: "boundary".parse().unwrap(),
            input: ConfigurationInput::Text {
                value: "periodic".into(),
            },
        });
    assert_eq!(
        initial
            .field(&selected, &incompatible, &vars, control())
            .unwrap_err(),
        InitializationError::Kernel(KernelRejection::Unsupported)
    );
    incompatible = request("newtonian");
    incompatible.domain.discretization = SpatialDiscretization::CartesianCells { cells: [4; 3] };
    assert!(matches!(
        initial.field(&selected, &incompatible, &vars, control()),
        Err(InitializationError::Kernel(_))
    ));
    incompatible = request("newtonian");
    incompatible
        .configuration
        .push(AuthoredConfigurationProperty {
            id: "capacity".parse().unwrap(),
            input: ConfigurationInput::Expression {
                source: "2 kg".into(),
            },
        });
    assert!(matches!(
        initial.field(&selected, &incompatible, &vars, control()),
        Err(InitializationError::Declaration(_))
    ));
    assert_eq!(field.state_identity(), &before);
    assert_eq!(
        installed.store.list().unwrap().revision,
        selected.inventory_revision()
    );
}

#[test]
fn real_captures_enter_document_authority_atomically_and_undo_restores_bytes() {
    use kagami_document::scientific::{ScientificError, ScientificLimits, ScientificSetup};
    use kagami_document::{
        DisplayName, ExperimentCommand as Edit, ObjectSpec, TimeStep, WireSnapshot,
    };
    use kagami_session::{
        ActorId, CommandId, DocumentAuthority, ExperimentCommandEnvelope, SessionCommand,
    };
    let installed = Installed::new();
    let selected = installed.prepare();
    let schemas = installed.store.available_components(&[]).unwrap().schemas;
    let initial = initializer(1024 * 1024);
    let vars = VariablesSystem::default();
    let field_request = request("newtonian");
    let field = initial
        .field(&selected, &field_request, &vars, control())
        .unwrap();
    let mut empty = Vec::new();
    encode_batch::<DynamicEntity>(&[], &mut empty, BulkLimits::default()).unwrap();
    let history = initial
        .history(
            &selected,
            &request("euler"),
            &vars,
            Buffer {
                schema: DynamicEntity::SCHEMA.into(),
                value_count: 0,
                bytes: empty.into(),
            },
            control(),
        )
        .unwrap();
    let captures = vec![field.document_capture(), history.document_capture()];
    let scientific = Arc::new(
        ScientificSetup::capture(
            selected.compiled().verified(),
            field_request.domain.clone(),
            TimeStep::default(),
            captures.clone(),
            ScientificLimits::default(),
        )
        .unwrap(),
    );
    assert_window_reinitialization(&schemas, &scientific);
    let mut authority = DocumentAuthority::new(schemas.clone(), kagami_document::Limits::default());
    let envelope = |id: &str, command| {
        ExperimentCommandEnvelope::new(
            CommandId::new(id).unwrap(),
            ActorId::new("test").unwrap(),
            command,
        )
    };
    let initial_revision = authority.revision();
    let adoption = Edit::AdoptScientificSetup(scientific.clone());
    let mut tight = DocumentAuthority::new(
        schemas.clone(),
        kagami_document::Limits {
            scientific: ScientificLimits {
                total_bytes: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert_eq!(
        tight
            .submit(envelope(
                "tight-adoption",
                SessionCommand::Edit(vec![adoption.clone()])
            ))
            .unwrap_err()
            .code(),
        "scientific_setup_limit"
    );
    assert!(!tight.is_dirty());
    let mut short_expression = DocumentAuthority::new(
        schemas.clone(),
        kagami_document::Limits {
            max_expression_bytes: 1,
            ..Default::default()
        },
    );
    assert_eq!(
        short_expression
            .submit(envelope(
                "tight-expression",
                SessionCommand::Edit(vec![adoption.clone()])
            ))
            .unwrap_err()
            .code(),
        "scientific_setup_limit"
    );
    assert!(
        kagami_document::WireCommand::of(&adoption).is_none(),
        "effect results cannot be smuggled through ordinary JSON intents"
    );
    authority
        .submit(
            envelope("adopt", SessionCommand::Edit(vec![adoption.clone()]))
                .guarded_by(initial_revision),
        )
        .unwrap();
    assert!(authority.is_dirty());
    assert!(authority.snapshot().setup().legacy().is_none());
    let description = serde_json::to_value(WireSnapshot::of(&authority.snapshot())).unwrap();
    assert_eq!(
        serde_json::from_value::<WireSnapshot>(description.clone()).unwrap(),
        WireSnapshot::of(&authority.snapshot())
    );
    assert_eq!(description["version"], 3);
    assert_eq!(
        description["setup"]["apiVersion"],
        "kagami.scientific-setup/v1"
    );
    assert!(description["setup"]["kernels"][0]["state"]["digest"].is_string());
    assert!(
        serde_json::to_vec(authority.snapshot().setup()).is_err(),
        "legacy JSON must not discard scientific blobs"
    );
    assert_eq!(
        authority
            .submit(
                envelope("stale", SessionCommand::Edit(vec![adoption]))
                    .guarded_by(initial_revision)
            )
            .unwrap_err()
            .code(),
        "revision_conflict"
    );
    authority
        .submit(envelope("undo", SessionCommand::Undo))
        .unwrap();
    assert_eq!(
        authority
            .snapshot()
            .setup()
            .legacy()
            .unwrap()
            .domain
            .cells(),
        [32; 3]
    );
    authority
        .submit(envelope("redo", SessionCommand::Redo))
        .unwrap();
    let snapshot = authority.snapshot();
    let restored = snapshot.setup().scientific().unwrap();
    assert!(Arc::ptr_eq(
        &restored.captures()[&"newtonian".parse().unwrap()].state,
        &field.state().bytes
    ));
    let revision = authority.revision();
    assert_eq!(
        authority
            .submit(envelope(
                "legacy-domain",
                SessionCommand::Edit(vec![Edit::SetDomain(Default::default())])
            ))
            .unwrap_err()
            .code(),
        "scientific_reinitialization_required"
    );
    assert_eq!(authority.revision(), revision);
    let dynamics = selected.compiled().verified().payloads().iter().find(|(_,p)| matches!(p, Payload::Components(c) if c.scientific.role == ComponentRole::Dynamics)).unwrap().0;
    let component = kagami_catalog::ComponentTypeId::exact(dynamics.clone()).unwrap();
    let schema = schemas.get(&component).unwrap();
    let object = ObjectSpec::new(DisplayName::new("dynamic").unwrap())
        .with_component(component, component_defaults(schema));
    assert_eq!(
        authority
            .submit(envelope(
                "stale-history",
                SessionCommand::Edit(vec![Edit::CreateObject(Box::new(object.clone()))])
            ))
            .unwrap_err()
            .code(),
        "scientific_history_changed"
    );
    assert_eq!(authority.snapshot().object_count(), 0);
    assert_eq!(authority.revision(), revision);
    authority
        .submit(envelope(
            "dt",
            SessionCommand::Edit(vec![Edit::SetTimeStep(TimeStep::new(0.002).unwrap())]),
        ))
        .unwrap();
    assert_eq!(authority.snapshot().setup().time_step().seconds(), 0.002);
    let mut corrupt = captures.clone();
    corrupt[0].configuration = Arc::from(&b"wrong"[..]);
    assert_eq!(
        ScientificSetup::capture(
            selected.compiled().verified(),
            field_request.domain.clone(),
            TimeStep::default(),
            corrupt,
            ScientificLimits::default()
        )
        .unwrap_err(),
        ScientificError::Mismatch
    );
    assert_eq!(
        ScientificSetup::capture(
            selected.compiled().verified(),
            field_request.domain.clone(),
            TimeStep::default(),
            captures.clone(),
            ScientificLimits {
                total_bytes: 1,
                ..Default::default()
            }
        )
        .unwrap_err(),
        ScientificError::Limit
    );
    let mut changed = captures;
    changed[0].authored.push(AuthoredConfigurationProperty {
        id: "capacity".parse().unwrap(),
        input: ConfigurationInput::Expression {
            source: "32".into(),
        },
    });
    let changed = ScientificSetup::capture(
        selected.compiled().verified(),
        field_request.domain,
        TimeStep::default(),
        changed,
        ScientificLimits::default(),
    )
    .unwrap();
    let revision = authority.revision();
    assert_eq!(
        authority
            .submit(envelope(
                "changed-parameter",
                SessionCommand::Edit(vec![Edit::AdoptScientificSetup(Arc::new(changed))])
            ))
            .unwrap_err()
            .code(),
        "scientific_reinitialization_required"
    );
    assert_eq!(authority.revision(), revision);

    // A variable and the matching frozen configuration are one edit. A later
    // value change cannot leave the old initialized field masquerading as new.
    let mut variable_field = field.document_capture();
    variable_field.authored.push(AuthoredConfigurationProperty {
        id: "capacity".parse().unwrap(),
        input: ConfigurationInput::Expression {
            source: "capacity".into(),
        },
    });
    let variable_setup = ScientificSetup::capture(
        selected.compiled().verified(),
        request("newtonian").domain,
        TimeStep::default(),
        vec![variable_field, history.document_capture()],
        ScientificLimits::default(),
    )
    .unwrap();
    authority
        .submit(envelope(
            "bind-variable",
            SessionCommand::Edit(vec![
                Edit::DefineVariable(Box::new(kagami_document::VariableSpec::new(
                    orishu_variables::Name::new("capacity").unwrap(),
                    "64",
                ))),
                Edit::AdoptScientificSetup(Arc::new(variable_setup)),
            ]),
        ))
        .unwrap();
    let variable = *authority.snapshot().variables().keys().next().unwrap();
    let revision = authority.revision();
    assert_eq!(
        authority
            .submit(envelope(
                "change-variable",
                SessionCommand::Edit(vec![Edit::SetVariableExpression {
                    variable,
                    expression: "32".into()
                },])
            ))
            .unwrap_err()
            .code(),
        "scientific_reinitialization_required"
    );
    assert_eq!(authority.revision(), revision);

    // Final-candidate validation permits an object creation with its newly
    // initialized history in the same batch; no partially updated revision exists.
    let zero = FiniteF64::new(0.0).unwrap();
    let dynamic = DynamicEntity {
        id: EntityId(0),
        kinematics: Kinematics {
            position_metres: [zero; 3],
            velocity_metres_per_second: [zero; 3],
        },
        inertial_mass_kilograms: FiniteF64::new(1.0).unwrap(),
    };
    let mut bytes = Vec::new();
    encode_batch(&[dynamic], &mut bytes, BulkLimits::default()).unwrap();
    let populated_history = initial
        .history(
            &selected,
            &request("euler"),
            &vars,
            Buffer {
                schema: DynamicEntity::SCHEMA.into(),
                value_count: 1,
                bytes: bytes.into(),
            },
            control(),
        )
        .unwrap();
    let populated = ScientificSetup::capture(
        selected.compiled().verified(),
        request("newtonian").domain,
        TimeStep::default(),
        vec![
            field.document_capture(),
            populated_history.document_capture(),
        ],
        ScientificLimits::default(),
    )
    .unwrap();
    authority
        .submit(envelope(
            "create-with-history",
            SessionCommand::Edit(vec![
                Edit::CreateObject(Box::new(object)),
                Edit::AdoptScientificSetup(Arc::new(populated)),
            ]),
        ))
        .unwrap();
    assert_eq!(authority.snapshot().object_count(), 1);

    // Unsupported candidates refuse before temporary creation/target replacement.
    use kagami_session::{
        default_view::AuthoringView,
        document::{DocumentMetadata, ExperimentDocument},
        persist::DocumentTarget,
        store::{self, RealFileStore},
    };
    let document = ExperimentDocument::of(
        authority.experiment(),
        &authority.snapshot(),
        &AuthoringView::default(),
        DocumentMetadata {
            generator: "test".into(),
            created: "test".into(),
            saved: "test".into(),
            saved_revision: authority.revision().get(),
        },
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("experiment.kagami");
    std::fs::write(&path, b"previous file").unwrap();
    let mut unsupported = document.clone();
    unsupported.format_version = 99;
    assert!(
        store::save(
            &RealFileStore,
            &DocumentTarget::new(path.clone()).unwrap(),
            &unsupported
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"previous file");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    // The production file store selects the scientific container and restores
    // exact captured bytes without any installed provider or initializer.
    use kagami_session::container::{self, ContainerLimits};
    use orishu_plugin::archive::{self, Root};
    let limits = ContainerLimits::default();
    let encoded = container::encode(&document, limits).unwrap();
    let decoded = container::decode(&encoded, limits).unwrap();
    assert_eq!(decoded, document);
    assert_eq!(container::encode(&decoded, limits).unwrap(), encoded);
    let target = DocumentTarget::new(path.clone()).unwrap();
    store::save(&RealFileStore, &target, &document).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), encoded);
    let loaded = store::load(&RealFileStore, &target).unwrap();
    assert_eq!(loaded.from, kagami_session::LoadedFrom::Primary);
    assert_eq!(loaded.document, document);
    // A second save retains a readable scientific backup through the same path.
    store::save(&RealFileStore, &target, &document).unwrap();
    assert_eq!(
        std::fs::read(path.with_extension("kagami.bak")).unwrap(),
        encoded
    );
    let archive = archive::read(&encoded, Root::Document, limits.archive).unwrap();
    let json: serde_json::Value = serde_json::from_slice(archive.root).unwrap();
    assert_eq!(json["formatVersion"], 4);
    for payload in selected.compiled().verified().payloads().values() {
        if let Some(kernel) = payload.kernel() {
            assert!(!archive.blobs.contains_key(&kernel));
        }
    }
    // Repack through valid framing so semantic corruption cannot be dismissed
    // merely because a ZIP checksum no longer matches.
    let repack = |json: &serde_json::Value, blobs: &std::collections::BTreeMap<_, _>| {
        archive::pack(
            Root::Document,
            &serde_json::to_vec(json).unwrap(),
            blobs,
            limits.archive,
        )
        .unwrap()
    };
    let mut future = json.clone();
    future["formatVersion"] = 5.into();
    let refused = container::decode(&repack(&future, &archive.blobs), limits).unwrap_err();
    assert_eq!(refused.code(), "unsupported_format_version");
    assert!(!refused.is_damage());
    let newer = repack(&future, &archive.blobs);
    std::fs::write(&path, &newer).unwrap();
    assert_eq!(
        store::load(&RealFileStore, &target).unwrap_err().code(),
        "unsupported_format_version"
    );
    assert_eq!(std::fs::read(&path).unwrap(), newer);
    // CRC corruption, unlike a declined version, permits verified backup recovery.
    let mut corrupt = encoded.clone();
    corrupt[43] ^= 1; // First root byte; framing and root name remain intact.
    std::fs::write(&path, &corrupt).unwrap();
    let recovered = store::load(&RealFileStore, &target).unwrap();
    assert_eq!(recovered.from, kagami_session::LoadedFrom::Backup);
    assert_eq!(recovered.document, document);
    let mut forged = json.clone();
    forged["experiment"]["setup"]["kernels"][0]["state"]["byteLength"] = 0.into();
    assert!(container::decode(&repack(&forged, &archive.blobs), limits).is_err());
    let mut forged = json.clone();
    forged["experiment"]["setup"]["kernels"][0]["state"]["schema"] = "fake/v1".into();
    assert!(container::decode(&repack(&forged, &archive.blobs), limits).is_err());
    let mut extra = archive.blobs.clone();
    extra.insert(ArtifactDigest::sha256_of(b"unrelated"), b"unrelated");
    assert!(container::decode(&repack(&json, &extra), limits).is_err());
    for digest in archive.blobs.keys() {
        let mut missing = archive.blobs.clone();
        missing.remove(digest);
        assert!(container::decode(&repack(&json, &missing), limits).is_err());
    }
    let mut policy = limits;
    policy.scientific.total_bytes = 1;
    assert_eq!(
        container::decode(&encoded, policy).unwrap_err().code(),
        "scientific_setup_limit"
    );
    policy = limits;
    policy.archive.max_bytes = encoded.len() - 1;
    assert!(container::decode(&encoded, policy).is_err());
    policy = limits;
    policy.archive.root_bytes = archive.root.len() - 1;
    assert!(container::decode(&encoded, policy).is_err());
    assert!(container::encode(&document, policy).is_err());
    let reopened = decoded
        .into_experiment(
            &kagami_catalog::SchemaRegistry::new(),
            &kagami_document::Limits::default(),
        )
        .unwrap();
    assert_eq!(reopened.snapshot().object_count(), 1);
    assert!(reopened.snapshot().setup().scientific().is_some());
    assert_retention_admission(&document, &schemas);
    assert_document_projection(&reopened, &selected, &schemas);
    // Missing schemas must not turn an unknown separate unit into bare SI.
    let mut bad_unit = document.clone();
    let kagami_session::document::StoredValue::Quantity { unit, .. } =
        bad_unit.experiment.objects[0].components[0]
            .properties
            .values_mut()
            .next()
            .unwrap()
    else {
        panic!("reference mass is a quantity")
    };
    *unit = Some("not-a-known-unit".into());
    if let Ok(invalid) = bad_unit.into_experiment(&Default::default(), &Default::default()) {
        assert!(
            kagami_document::projection::project(
                &invalid.snapshot(),
                Default::default(),
                &Default::default()
            )
            .is_err()
        );
    }
}

fn assert_window_reinitialization(
    schemas: &kagami_catalog::SchemaRegistry,
    scientific: &Arc<kagami_document::scientific::ScientificSetup>,
) {
    use kagami::{
        launch::LaunchOptions,
        message::{Message, ScientificAction},
        model::Model,
        scientific_effect::ScientificPlugins,
        update::update,
    };
    use kagami_document::ExperimentCommand;
    use kagami_session::SessionCommand;
    let installed = Installed::new();
    let store = Arc::new(PluginStore::open(installed._dir.path()).unwrap());
    let inventory = ScientificPlugins {
        store: store.clone(),
        revision: store.list().unwrap().revision,
        overrides: vec![],
    };
    let mut model = Model::new(LaunchOptions {
        plugin_schemas: schemas.clone(),
        scientific_plugins: Some(inventory.clone()),
        ..Default::default()
    });
    assert!(
        model
            .document
            .edit(vec![ExperimentCommand::AdoptScientificSetup(
                scientific.clone()
            )])
    );
    let history_id: orishu_workload::ComponentInstanceId = "euler".parse().unwrap();
    let field_id: orishu_workload::ComponentInstanceId = "newtonian".parse().unwrap();
    let before = model.document.snapshot().clone();
    let run = |model: &mut Model| {
        let _ = update(
            model,
            Message::Scientific(ScientificAction::Reinitialize(field_id.clone())),
        );
        assert_eq!(model.queue_len, 1);
        let until = std::time::Instant::now() + Duration::from_secs(75);
        while model.scientific_effects.is_pending() {
            let _ = update(model, Message::Scientific(ScientificAction::Poll));
            assert!(
                std::time::Instant::now() < until,
                "background job exceeded the test deadline"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    run(&mut model);
    assert_eq!(model.queue_len, 0);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let after = model.document.snapshot().clone();
    assert_eq!(after.revision().get(), before.revision().get() + 1);
    let old = before.setup().scientific().unwrap();
    let new = after.setup().scientific().unwrap();
    assert_eq!(
        new.captures()[&history_id],
        old.captures()[&history_id],
        "field reset must not reinitialize dynamics history"
    );
    assert_eq!(
        new.captures()[&field_id].state,
        old.captures()[&field_id].state,
        "Newtonian natural state is deterministic"
    );
    assert_eq!(
        new.declarations().descriptor(),
        old.declarations().descriptor()
    );
    assert!(model.document.submit(SessionCommand::Undo));
    assert_eq!(model.document.snapshot().setup().scientific(), Some(old));
    assert!(model.document.submit(SessionCommand::Redo));
    assert_eq!(model.document.snapshot().setup().scientific(), Some(new));

    // Stale completion may not replace a newly opened/created document.
    let _ = update(
        &mut model,
        Message::Scientific(ScientificAction::Reinitialize(field_id.clone())),
    );
    assert!(model.document.new_experiment(true));
    let replacement = model.document.view().revision();
    let until = std::time::Instant::now() + Duration::from_secs(75);
    while model.scientific_effects.is_pending() {
        let _ = update(&mut model, Message::Scientific(ScientificAction::Poll));
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(model.document.view().revision(), replacement);
    assert!(model.document.snapshot().setup().scientific().is_none());

    // Availability guard prevents a concurrent CLI writer from slipping between
    // final inventory validation and document adoption; losing that race refuses.
    let enable = InventoryCommand::SetEnabled {
        plugin_id: "org.orishu.reference.vocabulary".parse().unwrap(),
        enabled: false,
    };
    let guard = store.guard_revision(inventory.revision).unwrap();
    assert_eq!(
        store
            .submit(inventory.revision, enable.clone())
            .unwrap_err()
            .code,
        Code::Busy
    );
    drop(guard);
    let disabled = store.submit(inventory.revision, enable).unwrap();
    assert_eq!(
        store.guard_revision(inventory.revision).unwrap_err().code,
        Code::StaleRevision
    );
    assert!(
        model
            .document
            .edit(vec![ExperimentCommand::AdoptScientificSetup(
                scientific.clone()
            )])
    );
    let refused = model.document.view().revision();
    run(&mut model);
    assert_eq!(model.document.view().revision(), refused);
    assert!(model.document.notice.is_some());
    // Restore the fixture's inventory state for its remaining independent checks.
    store
        .submit(
            disabled,
            InventoryCommand::SetEnabled {
                plugin_id: "org.orishu.reference.vocabulary".parse().unwrap(),
                enabled: true,
            },
        )
        .unwrap();
}

#[test]
fn explicit_setup_creation_captures_existing_dynamics_atomically() {
    use kagami::{
        launch::LaunchOptions,
        message::{Message, ScientificAction},
        model::Model,
        scientific_effect::{ScientificPlugins, SetupRequest},
        update::update,
    };
    use kagami_document::{
        DisplayName, ExperimentCommand as Edit, ObjectSpec, TimeStep, Transform, Vector3, Velocity,
    };
    let installed = Installed::new();
    let selected = installed.prepare();
    let available = installed.store.available_components(&[]).unwrap();
    let dynamics = selected.compiled().verified().payloads().iter().find(|(_,p)| matches!(p, Payload::Components(c) if c.scientific.role == ComponentRole::Dynamics)).unwrap().0;
    let kind = kagami_catalog::ComponentTypeId::exact(dynamics.clone()).unwrap();
    let mut model = Model::new(LaunchOptions {
        plugin_schemas: available.schemas.clone(),
        scientific_plugins: Some(ScientificPlugins {
            store: Arc::new(PluginStore::open(installed._dir.path()).unwrap()),
            revision: available.revision,
            overrides: vec![],
        }),
        ..Default::default()
    });
    let object = ObjectSpec::new(DisplayName::new("dynamic").unwrap())
        .with_component(
            kind.clone(),
            component_defaults(available.schemas.get(&kind).unwrap()),
        )
        .with_transform(Transform {
            translation: Vector3::new(3.0, 4.0, 5.0).unwrap(),
            ..Default::default()
        })
        .with_velocity(Velocity {
            linear: Vector3::new(1.0, 2.0, 3.0).unwrap(),
            ..Default::default()
        });
    assert!(model.document.edit(vec![
        Edit::CreateObject(Box::new(object)),
        Edit::CreateObject(Box::new(ObjectSpec::new(
            DisplayName::new("static").unwrap()
        )))
    ]));
    let before = model.document.snapshot().clone();
    let proposal = SetupRequest {
        selection: installed.request.clone(),
        kernels: installed.uses.clone(),
        initialization: vec![request("euler"), request("newtonian")],
        time_step: TimeStep::default(),
    };
    let finish = |model: &mut Model| {
        let until = std::time::Instant::now() + Duration::from_secs(75);
        while model.scientific_effects.is_pending() {
            let _ = update(model, Message::Scientific(ScientificAction::Poll));
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let _ = update(
        &mut model,
        Message::Scientific(ScientificAction::Configure(Box::new(proposal.clone()))),
    );
    assert_eq!(model.document.snapshot().revision(), before.revision());
    assert!(model.scientific_effects.is_pending());
    finish(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let after = model.document.snapshot().clone();
    assert_eq!(after.revision().get(), before.revision().get() + 1);
    assert_eq!(after.objects(), before.objects());
    let setup = after.setup().scientific().unwrap();
    let history = setup
        .captures()
        .values()
        .find(|c| c.context.execution_contract == ExecutionContractId::Dynamics)
        .unwrap();
    let packet = Batch::<DynamicEntity>::read(
        history.history_entities.as_ref().unwrap(),
        BulkLimits::default(),
    )
    .unwrap();
    assert_eq!(packet.len(), 1);
    assert_eq!(
        packet
            .get(0)
            .unwrap()
            .kinematics
            .position_metres
            .map(FiniteF64::get),
        [3.0, 4.0, 5.0]
    );
    assert_eq!(
        packet
            .get(0)
            .unwrap()
            .kinematics
            .velocity_metres_per_second
            .map(FiniteF64::get),
        [1.0, 2.0, 3.0]
    );
    let vars = kagami_document::variable_context(&after, model.document.limits()).unwrap();
    assert!(
        kagami_document::scientific::initial_dynamics(
            &after,
            setup.declarations(),
            &"euler".parse().unwrap(),
            &vars,
            model.document.limits(),
            BulkLimits {
                records: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
    // Actual composed scene is exportable, not merely an accepted opaque capture.
    let compiled = kagami::workload::compile_captured(
        &after,
        &selected,
        orishu_workload::WorkloadMeta::new("created".parse().unwrap()),
        Default::default(),
        model.document.limits(),
    )
    .unwrap();
    assert_eq!(compiled.projection().definition().fields.len(), 1);
    assert!(model.document.submit(kagami_session::SessionCommand::Undo));
    assert_eq!(model.document.snapshot().setup(), before.setup());
    assert!(model.document.submit(kagami_session::SessionCommand::Redo));
    assert_eq!(model.document.snapshot().setup(), after.setup());
    let stable = model.document.snapshot().clone();
    // Integrator construction succeeds before the later field rejects its config.
    let mut rejected = proposal;
    rejected.initialization[1]
        .configuration
        .push(AuthoredConfigurationProperty {
            id: "capacity".parse().unwrap(),
            input: ConfigurationInput::Expression {
                source: "1 kg".into(),
            },
        });
    let _ = update(
        &mut model,
        Message::Scientific(ScientificAction::Configure(Box::new(rejected))),
    );
    finish(&mut model);
    assert!(model.document.notice.is_some());
    assert_eq!(model.document.snapshot(), &stable);
}

#[test]
fn physics_form_creates_saves_and_exports_a_new_experiment() {
    use kagami::{
        launch::LaunchOptions,
        message::{Message, ScientificAction},
        model::Model,
        physics_form::{ParameterAction, PhysicsAction},
        scientific_effect::ScientificPlugins,
        update::update,
    };
    let installed = Installed::new();
    let available = installed.store.available_components(&[]).unwrap();
    let choices = installed.store.available_models(&[]).unwrap();
    assert_eq!(choices.revision, available.revision);
    assert_eq!(choices.kernels.len(), 2);
    assert!(
        installed
            .store
            .available_models(&[("org.orishu.reference.solvers".parse().unwrap(), false)])
            .unwrap()
            .kernels
            .is_empty()
    );
    let mut model = Model::new(LaunchOptions {
        plugin_schemas: available.schemas,
        kernel_choices: choices.kernels,
        scientific_plugins: Some(ScientificPlugins {
            store: Arc::new(PluginStore::open(installed._dir.path()).unwrap()),
            revision: choices.revision,
            overrides: vec![],
        }),
        ..Default::default()
    });
    for i in 0..model.kernel_choices.len() {
        let _ = update(&mut model, Message::PhysicsForm(PhysicsAction::Kernel(i)));
    }
    let gravity = model
        .kernel_choices
        .iter()
        .position(|k| k.contract == ExecutionContractId::Field)
        .unwrap();
    let parameter = |model: &mut Model, source: &str| {
        let _ = update(
            model,
            Message::PhysicsForm(PhysicsAction::Parameter(ParameterAction {
                kernel: gravity,
                property: "exclusion-radius".parse().unwrap(),
                input: Some(ConfigurationInput::Expression {
                    source: source.into(),
                }),
            })),
        );
    };
    parameter(&mut model, "2 mm");
    // Nothing selects or initializes until both explicit selection and reset consent.
    let _ = update(&mut model, Message::PhysicsForm(PhysicsAction::Apply));
    assert!(!model.scientific_effects.is_pending());
    assert!(model.document.snapshot().setup().scientific().is_none());
    let _ = update(
        &mut model,
        Message::PhysicsForm(PhysicsAction::Confirm(true)),
    );
    let _ = update(
        &mut model,
        Message::PhysicsForm(PhysicsAction::Lower(0, "NaN".into())),
    );
    assert!(!model.physics_form.confirmed);
    model.physics_form.confirmed = true;
    assert!(
        model
            .physics_form
            .request(&model.kernel_choices, choices.revision)
            .is_err()
    );
    let _ = update(
        &mut model,
        Message::PhysicsForm(PhysicsAction::Lower(0, "-10".into())),
    );
    let _ = update(
        &mut model,
        Message::PhysicsForm(PhysicsAction::Confirm(true)),
    );
    let _ = update(&mut model, Message::PhysicsForm(PhysicsAction::Apply));
    assert!(model.scientific_effects.is_pending());
    assert!(!model.physics_form.confirmed);
    let until = std::time::Instant::now() + Duration::from_secs(75);
    while model.scientific_effects.is_pending() {
        let _ = update(&mut model, Message::Scientific(ScientificAction::Poll));
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    assert_eq!(
        model
            .document
            .snapshot()
            .setup()
            .scientific()
            .unwrap()
            .captures()
            .len(),
        2
    );
    let initial = model.document.snapshot().clone();
    let first = initial.setup().scientific().unwrap();
    let field_id = first
        .captures()
        .iter()
        .find(|(_, c)| c.context.execution_contract == ExecutionContractId::Field)
        .unwrap()
        .0
        .clone();
    let assert_radius = |capture: &kagami_document::scientific::KernelCapture,
                         source: &str,
                         radius: f64| {
        assert!(
            capture
                .authored
                .iter()
                .any(|p| p.id.as_str() == "exclusion-radius"
                    && p.input
                        == ConfigurationInput::Expression {
                            source: source.into()
                        })
        );
        let resolved =
            ResolvedConfiguration::from_cbor(&capture.configuration, &Default::default()).unwrap();
        assert!(
            matches!(resolved.get("exclusion-radius"), Some(ConfigurationValue::Quantity { value_si, dimension }) if value_si.get() == radius && *dimension == orishu_variables::Dimension::LENGTH)
        );
    };
    assert_radius(&first.captures()[&field_id], "2 mm", 0.002);
    let _ = update(
        &mut model,
        Message::PhysicsForm(PhysicsAction::LoadCaptured),
    );
    assert_eq!(model.document.snapshot(), &initial);
    assert!(!model.scientific_effects.is_pending());
    assert!(model.physics_form.is_captured());
    assert!(!model.physics_form.confirmed);
    let _ = update(
        &mut model,
        Message::PhysicsForm(PhysicsAction::Confirm(true)),
    );
    let copied = model
        .physics_form
        .request(&model.kernel_choices, choices.revision)
        .unwrap();
    assert_eq!(
        copied.kernels,
        first.declarations().descriptor().kernel_instances
    );
    assert_eq!(
        copied.selection.roots,
        first.declarations().descriptor().roots
    );
    assert_eq!(
        copied.selection.bindings.len(),
        first.declarations().descriptor().bindings.len()
    );
    for binding in &first.declarations().descriptor().bindings {
        assert!(
            copied
                .selection
                .bindings
                .iter()
                .any(|b| b.requirement.consumer == binding.consumer
                    && b.requirement.slot == binding.requirement_slot
                    && b.provider == binding.provider)
        );
    }
    assert!(
        kagami::physics_form::PhysicsForm::from_captured(first, &[], choices.revision).is_err()
    );
    assert!(
        model
            .physics_form
            .request(&model.kernel_choices, choices.revision + 1)
            .is_err()
    );
    let selected_before = model.physics_form.selected.clone();
    let _ = update(
        &mut model,
        Message::PhysicsForm(PhysicsAction::Kernel(gravity)),
    );
    assert_eq!(
        model.physics_form.selected, selected_before,
        "captured model pins cannot silently change"
    );
    for request in &copied.initialization {
        let capture = &first.captures()[&request.instance];
        assert_eq!(request.configuration, capture.authored);
        assert_eq!(request.compute_precision, capture.context.compute_precision);
        assert_eq!(
            request.quality_flags,
            capture
                .context
                .observables
                .iter()
                .map(|o| (o.slot.clone(), o.quality_flags))
                .collect::<std::collections::BTreeMap<_, _>>()
        );
    }
    // A parameter with the wrong dimension is accepted only as local text. The
    // shared compiler refuses it; neither initialized fields nor history leak in.
    parameter(&mut model, "2 kg");
    assert!(!model.physics_form.confirmed);
    let apply = |model: &mut Model| {
        let _ = update(model, Message::PhysicsForm(PhysicsAction::Confirm(true)));
        let _ = update(model, Message::PhysicsForm(PhysicsAction::Apply));
        assert!(model.scientific_effects.is_pending());
        let until = std::time::Instant::now() + Duration::from_secs(75);
        while model.scientific_effects.is_pending() {
            let _ = update(model, Message::Scientific(ScientificAction::Poll));
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    apply(&mut model);
    assert_eq!(model.document.snapshot(), &initial);
    assert!(model.document.notice.is_some());
    parameter(&mut model, "4 mm");
    apply(&mut model);
    assert!(
        model.document.notice.is_none(),
        "{:?}",
        model.document.notice
    );
    let edited = model.document.snapshot().clone();
    let setup = edited.setup().scientific().unwrap();
    assert_eq!(
        setup.declarations().descriptor(),
        first.declarations().descriptor()
    );
    assert_radius(&setup.captures()[&field_id], "4 mm", 0.004);
    assert_eq!(edited.revision().get(), initial.revision().get() + 1);
    assert!(model.document.submit(kagami_session::SessionCommand::Undo));
    assert_eq!(model.document.snapshot().setup().scientific(), Some(first));
    assert!(model.document.submit(kagami_session::SessionCommand::Redo));
    assert_eq!(model.document.snapshot().setup().scientific(), Some(setup));
    let input = installed._dir.path().join("new-experiment.kagami");
    assert!(model.document.save(Some(input.clone()), "test".into()));
    let output = installed._dir.path().join("new-experiment.orishu");
    let report = kagami::export::execute(kagami::export::ExportArgs {
        experiment: input.clone(),
        output: output.clone(),
        name: "ui-created".parse().unwrap(),
        directory: Some(installed._dir.path().to_path_buf()),
        expected_inventory_revision: Some(choices.revision),
        enable_plugin: vec![],
        disable_plugin: vec![],
        json: false,
    })
    .unwrap();
    let bytes = std::fs::read(output).unwrap();
    let bundle =
        orishu_plugin::workload::bundle::read(&bytes, Default::default(), Default::default())
            .unwrap();
    assert_eq!(bundle.verified().root(), report.workload);
    assert!(model.document.open(input, true));
    assert_radius(
        &model
            .document
            .snapshot()
            .setup()
            .scientific()
            .unwrap()
            .captures()[&field_id],
        "4 mm",
        0.004,
    );
}

fn assert_document_projection(
    reopened: &kagami_document::Experiment,
    selected: &PreparedSelection,
    schemas: &kagami_catalog::SchemaRegistry,
) {
    use kagami_document::{
        ExperimentCommand as Edit,
        projection::{self, ProjectionError},
    };
    use orishu_plugin::workload::ProfileLimits;
    let policy = ProfileLimits::default();
    let bounds = kagami_document::Limits::default();
    let snapshot = reopened.snapshot();
    let projected = projection::project(&snapshot, policy, &bounds).unwrap();
    assert_eq!(projected.source(), &snapshot);
    let setup = snapshot.setup().scientific().unwrap();
    for field in &projected.definition().fields {
        assert!(Arc::ptr_eq(
            &projected.blobs()[&field.kernel.state.digest],
            &setup.captures()[&field.kernel.instance].state
        ));
    }
    for policy in [
        ProfileLimits {
            objects: 0,
            ..policy
        },
        ProfileLimits {
            fields: 0,
            ..policy
        },
        ProfileLimits {
            input_bytes: 1,
            ..policy
        },
    ] {
        assert!(matches!(
            projection::project(&snapshot, policy, &bounds),
            Err(ProjectionError::Limit)
        ));
    }
    assert!(matches!(
        projection::project(
            &kagami_document::Experiment::new().snapshot(),
            policy,
            &bounds
        ),
        Err(ProjectionError::LegacySetup)
    ));

    // Reopened without an installed schema: the cached SI values are unresolved.
    // Reprojection uses retained exact declarations and authored unit expressions.
    assert!(
        snapshot
            .objects()
            .values()
            .next()
            .unwrap()
            .components
            .values()
            .next()
            .unwrap()
            .properties
            .values()
            .all(|v| !v.is_priced())
    );
    let mass = selected.compiled().verified().payloads().iter()
        .find(|(_, p)| matches!(p, Payload::Components(c) if c.scientific.role == ComponentRole::FieldCoupling)).unwrap().0;
    let mass = kagami_catalog::ComponentTypeId::exact(mass.clone()).unwrap();
    let props = std::collections::BTreeMap::from([
        (
            kagami_catalog::PropertyName::new("source").unwrap(),
            kagami_document::AuthoredValue::si("1000 g"),
        ),
        (
            kagami_catalog::PropertyName::new("response").unwrap(),
            kagami_document::AuthoredValue::si("2 kg"),
        ),
    ]);
    let static_object = kagami_document::ObjectSpec::new(
        kagami_document::DisplayName::new("source only static").unwrap(),
    )
    .with_transform(kagami_document::Transform {
        translation: kagami_document::Vector3::new(2.0, 0.0, 0.0).unwrap(),
        ..Default::default()
    })
    .with_component(mass.clone(), props.clone());
    let (experiment, _) = kagami_document::update(
        reopened,
        &[
            Edit::AttachComponent {
                object: *snapshot.objects().keys().next().unwrap(),
                component: mass,
                properties: props,
            },
            Edit::CreateObject(Box::new(static_object)),
        ],
        schemas,
        &bounds,
    )
    .unwrap()
    .adopt();
    let snapshot = experiment.snapshot();
    let compiled = kagami::workload::compile_captured(
        &snapshot,
        selected,
        orishu_workload::WorkloadMeta::new("document-projection".parse().unwrap()),
        policy,
        &bounds,
    )
    .unwrap();
    let numeric = compiled.projection();
    assert_scene_admission(&compiled, selected);
    let field = &numeric.definition().fields[0];
    let coupled = Batch::<CoupledEntity>::read(
        &numeric.blobs()[&field.coupled.digest],
        BulkLimits::default(),
    )
    .unwrap();
    assert_eq!(coupled.len(), 2);
    assert!(coupled.get(0).unwrap().has_dynamics);
    assert!(!coupled.get(1).unwrap().has_dynamics);
    assert_eq!(coupled.get(0).unwrap().source_si.unwrap().get(), 1.0);
    assert_eq!(coupled.get(0).unwrap().response_si.unwrap().get(), 2.0);
    assert!(
        projection::project(
            &snapshot,
            ProfileLimits {
                couplings: 1,
                ..policy
            },
            &bounds
        )
        .is_err()
    );
    assert!(
        !compiled
            .blobs()
            .contains_key(&ArtifactDigest::sha256_of(UNUSED))
    );
    let portable = assert_portable_export(&experiment, &compiled);
    let portable =
        orishu_plugin::workload::bundle::read(&portable, policy, Default::default()).unwrap();
    let mut run = orishu_runtime::admit(
        Arc::new(Sandbox::new(SandboxLimits::default()).unwrap()),
        portable.manifest_bytes(),
        portable.blobs(),
        orishu_runtime::RunScope {
            workload: compiled.compiled().verified().root(),
            run: ArtifactDigest::sha256_of(b"document projection test run"),
            epoch: 1,
        },
        &Default::default(),
        Default::default(),
        control(),
    )
    .unwrap()
    .run;
    assert_eq!(
        run.state().fields()[0].bytes.as_ref(),
        &*numeric.blobs()[&field.kernel.state.digest]
    );
    run.advance(control()).unwrap();
    let objects =
        Batch::<ObjectState>::read(&run.state().objects().bytes, BulkLimits::default()).unwrap();
    assert!(
        objects
            .get(0)
            .unwrap()
            .kinematics
            .velocity_metres_per_second[0]
            .get()
            > 0.0
    );
    assert_eq!(
        objects.get(1).unwrap().kinematics.position_metres[0].get(),
        2.0
    );
    assert!(objects.get(1).unwrap().inertial_mass_kilograms.is_none());
    assert_eq!(
        experiment.snapshot(),
        snapshot,
        "compilation never edits authoring state"
    );
    assert_additive_data_export(&experiment);
    let mut changed_selection = Installed::new();
    changed_selection.uses[0].instance_id = "another-integrator-use".parse().unwrap();
    let changed_selection = changed_selection.prepare();
    assert!(matches!(
        kagami::workload::compile_captured(
            &snapshot,
            &changed_selection,
            orishu_workload::WorkloadMeta::new("wrong-selection".parse().unwrap()),
            policy,
            &bounds,
        ),
        Err(kagami::workload::CompileError::SelectionMismatch)
    ));

    // A legal authored component outside this numerical profile must not vanish
    // at the compilation boundary, even if it has no properties today.
    let unselected = kagami_catalog::ComponentTypeId::new(
        kagami_catalog::PluginId::new("test.unselected").unwrap(),
        kagami_catalog::ComponentName::new("future_emitter").unwrap(),
    );
    let registry = schemas.clone().with(kagami_catalog::ComponentSchema::new(
        unselected.clone(),
        kagami_catalog::SchemaVersion(1),
    ));
    let (unsupported, _) = kagami_document::update(
        &experiment,
        &[Edit::CreateObject(Box::new(
            kagami_document::ObjectSpec::new(
                kagami_document::DisplayName::new("unsupported intent").unwrap(),
            )
            .with_component(unselected.clone(), Default::default()),
        ))],
        &registry,
        &bounds,
    )
    .unwrap()
    .adopt();
    assert!(
        matches!(projection::project(&unsupported.snapshot(), policy, &bounds),
        Err(ProjectionError::UnsupportedComponent { component, .. }) if component == unselected)
    );
}

/// Exercise the shipped binary, not a parallel test-only exporter. The returned
/// file is admitted by a fresh sandbox after this helper drops its inventory.
fn assert_portable_export(
    experiment: &kagami_document::Experiment,
    compiled: &kagami::workload::CapturedWorkload,
) -> Vec<u8> {
    use kagami_session::{
        default_view::AuthoringView,
        document::{DocumentMetadata, ExperimentDocument},
        persist::DocumentTarget,
        store::{self, RealFileStore},
    };
    use orishu_plugin::{
        archive::{self, Root},
        workload::{ProfileLimits, bundle},
    };
    let installed = Installed::new();
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("captured.kagami");
    let output = dir.path().join("simulation.orishu");
    let document = ExperimentDocument::of(
        experiment,
        &experiment.snapshot(),
        &AuthoringView::default(),
        DocumentMetadata {
            generator: "test".into(),
            created: "test".into(),
            saved: "test".into(),
            saved_revision: 0,
        },
    );
    store::save(
        &RealFileStore,
        &DocumentTarget::new(input.clone()).unwrap(),
        &document,
    )
    .unwrap();
    let source = std::fs::read(&input).unwrap();
    let invoke = |input: &std::path::Path, output: &std::path::Path, args: &[&str], ok: bool| {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_kagami"))
            .arg("export")
            .arg(input)
            .arg("--output")
            .arg(output)
            .args(["--name", "document-projection", "--directory"])
            .arg(installed._dir.path())
            .arg("--json")
            .args(args)
            .env_remove("KAGAMI_PLUGIN_DIR")
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            ok,
            "stdout={} stderr={}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let response: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(response["apiVersion"], "kagami.workload-export/v1");
        assert_eq!(response["ok"], ok);
        response
    };
    let result = invoke(&input, &output, &[], true);
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(result["result"]["bytes"], bytes.len());
    let policy = ProfileLimits::default();
    let archive_policy = archive::ArchiveLimits::default();
    let portable = bundle::read(&bytes, policy, archive_policy).unwrap();
    compare_portable_layouts(&portable, bytes.len());
    assert_eq!(
        portable.verified().root(),
        compiled.compiled().verified().root()
    );
    assert_eq!(
        portable.manifest_bytes(),
        compiled.compiled().manifest_bytes()
    );
    assert!(
        !portable
            .blobs()
            .contains_key(&ArtifactDigest::sha256_of(UNUSED))
    );
    let mut cache = portable.blobs().clone();
    cache.insert(ArtifactDigest::sha256_of(UNUSED), UNUSED);
    assert_eq!(
        bundle::pack(portable.manifest_bytes(), &cache, policy, archive_policy).unwrap(),
        bytes
    );
    let extra = archive::pack(
        Root::Workload,
        portable.manifest_bytes(),
        &cache,
        archive_policy,
    )
    .unwrap();
    assert!(bundle::read(&extra, policy, archive_policy).is_err());
    let mut missing = portable.blobs().clone();
    missing.pop_first();
    let missing = archive::pack(
        Root::Workload,
        portable.manifest_bytes(),
        &missing,
        archive_policy,
    )
    .unwrap();
    assert!(bundle::read(&missing, policy, archive_policy).is_err());
    assert!(
        bundle::pack(
            portable.manifest_bytes(),
            &Default::default(),
            policy,
            archive_policy
        )
        .is_err()
    );
    for root in [Root::Plugin, Root::Document] {
        let wrong = archive::pack(
            root,
            portable.manifest_bytes(),
            portable.blobs(),
            archive_policy,
        )
        .unwrap();
        assert!(bundle::read(&wrong, policy, archive_policy).is_err());
    }
    let mut corrupt = bytes.clone();
    corrupt[43] ^= 1;
    assert!(bundle::read(&corrupt, policy, archive_policy).is_err());
    assert!(bundle::read(&bytes[..bytes.len() - 1], policy, archive_policy).is_err());
    let small = archive::ArchiveLimits {
        max_bytes: bytes.len() - 1,
        ..archive_policy
    };
    assert!(bundle::read(&bytes, policy, small).is_err());
    assert!(bundle::pack(portable.manifest_bytes(), portable.blobs(), policy, small).is_err());
    assert_eq!(
        invoke(&input, &output, &[], false)["error"]["stage"],
        "publish"
    );
    assert_eq!(std::fs::read(&output).unwrap(), bytes);
    let linked = dir.path().join("output-link");
    std::os::unix::fs::symlink(&input, &linked).unwrap();
    assert_eq!(
        invoke(&input, &linked, &[], false)["error"]["stage"],
        "publish"
    );
    let absent = dir.path().join("refused.orishu");
    let malformed = dir.path().join("malformed.kagami");
    std::fs::write(&malformed, b"not a captured document").unwrap();
    std::fs::write(malformed.with_extension("kagami.bak"), &source).unwrap();
    assert_eq!(
        invoke(&malformed, &absent, &[], false)["error"]["stage"],
        "document"
    );
    assert!(
        !absent.exists(),
        "export must not silently recover a different input"
    );
    assert_eq!(
        invoke(&linked, &absent, &[], false)["error"]["stage"],
        "read"
    );
    assert_eq!(
        invoke(
            &input,
            &absent,
            &["--expected-inventory-revision", "0"],
            false
        )["error"]["code"],
        "stale_inventory_revision"
    );
    assert!(!absent.exists());
    let plugin_id: PluginId = "org.orishu.reference.solvers".parse().unwrap();
    for flags in [
        vec![
            "--enable-plugin",
            plugin_id.as_str(),
            "--disable-plugin",
            plugin_id.as_str(),
        ],
        vec!["--enable-plugin", "org.example.uninstalled"],
    ] {
        assert_eq!(
            invoke(&input, &absent, &flags, false)["error"]["code"],
            "InvalidSelection"
        );
        assert!(!absent.exists());
    }
    installed
        .store
        .submit(
            installed.store.list().unwrap().revision,
            InventoryCommand::SetEnabled {
                plugin_id: plugin_id.clone(),
                enabled: false,
            },
        )
        .unwrap();
    let unavailable = invoke(&input, &absent, &[], false);
    assert_eq!(unavailable["error"]["code"], "selection_unavailable");
    assert!(unavailable["error"]["resolution"].is_object());
    assert!(!absent.exists());
    invoke(
        &input,
        &absent,
        &["--enable-plugin", plugin_id.as_str()],
        true,
    );
    assert_eq!(std::fs::read(&absent).unwrap(), bytes);
    assert!(
        installed
            .store
            .list()
            .unwrap()
            .releases
            .iter()
            .filter(|r| r.plugin_id == plugin_id)
            .all(|r| !r.enabled)
    );
    assert_eq!(std::fs::read(&input).unwrap(), source);
    bytes
}

/// Reproducible framing-only comparison for ADR 0010. Same scientific bytes,
/// stored-ZIP headers, no compression. OCI's index describes a custom CBOR root,
/// not a container image; this does not claim OCI runtime interoperability.
fn compare_portable_layouts(bundle: &orishu_plugin::workload::bundle::Bundle<'_>, actual: usize) {
    let root = bundle.manifest_bytes();
    let blob_path_len = "blobs/sha256/".len() + 64;
    let stored_entry = |name_len: usize, bytes: usize| 76 + 2 * name_len + bytes;
    let common: usize = bundle
        .blobs()
        .values()
        .map(|b| stored_entry(blob_path_len, b.len()))
        .sum();
    let minimal = 22 + stored_entry("workload.cbor".len(), root.len()) + common;
    assert_eq!(
        minimal, actual,
        "size model agrees with actual exported archive"
    );
    let layout = br#"{"imageLayoutVersion":"1.0.0"}"#;
    let index = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.index.v1+json",
        "manifests": [{
            "mediaType": "application/vnd.orishu.workload.v3+cbor",
            "digest": ArtifactDigest::sha256_of(root).to_string(),
            "size": root.len(),
        }],
    }))
    .unwrap();
    let oci = 22
        + common
        + stored_entry(blob_path_len, root.len())
        + stored_entry("oci-layout".len(), layout.len())
        + stored_entry("index.json".len(), index.len());
    eprintln!(
        "portable layout comparison: {} blobs, root={} bytes, minimal={} bytes, OCI-layout={} bytes, delta={} bytes",
        bundle.blobs().len(),
        root.len(),
        minimal,
        oci,
        oci - minimal
    );
    assert!(oci > minimal);
}

fn assert_additive_data_export(experiment: &kagami_document::Experiment) {
    use kagami_document::{
        ExperimentCommand as Edit,
        scientific::{ScientificLimits, ScientificSetup},
    };
    let mut installed = Installed::new();
    let contribution = Payload::Components(Declaration {
        scientific: ComponentSchema {
            name: "org.example.annotation".parse().unwrap(),
            version: 1.try_into().unwrap(),
            requirements: vec![],
            properties: vec![
                Property {
                    id: "flag".parse().unwrap(),
                    required: true,
                    schema: PropertyType::Boolean { default: None },
                },
                Property {
                    id: "label".parse().unwrap(),
                    required: true,
                    schema: PropertyType::Text {
                        max_bytes: 128,
                        default: None,
                    },
                },
            ],
            role: ComponentRole::Data,
            bindings: Default::default(),
        },
        presentation: None,
    });
    let package = package(
        "org.example.annotation",
        vec![("annotation".parse().unwrap(), contribution)],
        &[],
    );
    let reference = package
        .release()
        .contribution_ref(&"annotation".parse().unwrap())
        .unwrap();
    installed.request.expected_inventory_revision = installed
        .store
        .install(
            installed.request.expected_inventory_revision,
            &package,
            None,
            None,
        )
        .unwrap();
    installed.request.roots.push(reference.clone());
    installed.request.roots.sort();
    let prepared = installed.prepare();
    let schemas = installed.store.available_components(&[]).unwrap().schemas;
    let snapshot = experiment.snapshot();
    let old = snapshot.setup().scientific().unwrap();
    let setup = ScientificSetup::capture(
        prepared.compiled().verified(),
        old.domain().clone(),
        old.time_step(),
        old.captures().values().cloned().collect(),
        ScientificLimits::default(),
    )
    .unwrap();
    let component = kagami_catalog::ComponentTypeId::exact(reference.clone()).unwrap();
    let properties = std::collections::BTreeMap::from([
        (
            kagami_catalog::PropertyName::new("flag").unwrap(),
            kagami_document::AuthoredValue::Boolean(false),
        ),
        (
            kagami_catalog::PropertyName::new("label").unwrap(),
            kagami_document::AuthoredValue::Text("independent data, not a force role".into()),
        ),
    ]);
    let (scene, _) = kagami_document::update(
        experiment,
        &[
            Edit::AdoptScientificSetup(Arc::new(setup)),
            Edit::AttachComponent {
                object: *snapshot.objects().keys().next().unwrap(),
                component,
                properties,
            },
        ],
        &schemas,
        &Default::default(),
    )
    .unwrap()
    .adopt();
    let exported = kagami::workload::compile_captured(
        &scene.snapshot(),
        &prepared,
        orishu_workload::WorkloadMeta::new("additive-data".parse().unwrap()),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    let scene = exported.compiled().verified().scene().unwrap();
    let component = scene.objects[0]
        .components
        .iter()
        .find(|c| c.contribution == reference)
        .unwrap();
    assert_eq!(
        component.properties[0].value,
        ConfigurationValue::Boolean { value: false }
    );
    assert_eq!(
        component.properties[1].value,
        ConfigurationValue::Text {
            value: "independent data, not a force role".into()
        }
    );
    assert!(
        exported
            .compiled()
            .verified()
            .selection()
            .descriptor()
            .contributions
            .contains(&reference)
    );
}

fn assert_scene_admission(
    compiled: &kagami::workload::CapturedWorkload,
    selected: &PreparedSelection,
) {
    let scene = compiled
        .compiled()
        .verified()
        .scene()
        .expect("authored scene is verified");
    assert_eq!(scene.objects.len(), 2);
    assert_eq!(scene.variables[0].name, "capacity");
    assert_eq!(scene.variables[0].expression, "64");
    let source = scene.objects[0]
        .components
        .iter()
        .flat_map(|c| &c.properties)
        .find(|p| p.id.as_str() == "source")
        .unwrap();
    assert_eq!(source.source.as_ref().unwrap().expression, "1000 g");
    let recompile = |scene: &SceneDefinition, mut execution: ExecutionDefinition| {
        let bytes = scene.to_cbor(Default::default()).unwrap();
        let id = InputIdentity::of(SCENE_SCHEMA.parse().unwrap(), 1, &bytes);
        execution.scene = Some(id.clone());
        let mut blobs: std::collections::BTreeMap<_, _> = compiled
            .blobs()
            .iter()
            .map(|(d, b)| (*d, b.as_ref()))
            .collect();
        blobs.insert(id.digest, &bytes);
        orishu_plugin::workload::compile(
            orishu_workload::WorkloadMeta::new("scene-admission".parse().unwrap()),
            selected.compiled(),
            &execution,
            &blobs,
            Default::default(),
        )
    };
    for case in 0..8 {
        let mut scene = scene.clone();
        let mut execution = compiled.projection().definition().clone();
        match case {
            0 => {
                scene.objects[0]
                    .components
                    .iter_mut()
                    .flat_map(|c| &mut c.properties)
                    .find(|p| p.id.as_str() == "inertial-mass")
                    .unwrap()
                    .value = ConfigurationValue::Quantity {
                    value_si: FiniteF64::new(2.0).unwrap(),
                    dimension: Dimension::MASS,
                }
            }
            1 => scene.objects[0]
                .components
                .retain(|c| !c.properties.iter().any(|p| p.id.as_str() == "source")),
            2 => {
                scene.objects[0]
                    .components
                    .iter_mut()
                    .flat_map(|c| &mut c.properties)
                    .find(|p| p.id.as_str() == "source")
                    .unwrap()
                    .value = ConfigurationValue::Quantity {
                    value_si: FiniteF64::new(1.0).unwrap(),
                    dimension: Dimension::LENGTH,
                }
            }
            3 => scene.objects[0].kinematics.position_metres[0] = FiniteF64::new(3.0).unwrap(),
            4 => scene.objects[0].angular_velocity[0] = FiniteF64::new(1.0).unwrap(),
            5 => {
                scene.objects.pop();
            }
            6 => {
                scene.kernels.pop();
            }
            _ => execution.api_version = EXECUTION_SCHEMA.parse().unwrap(),
        }
        assert!(
            recompile(&scene, execution).is_err(),
            "scene mismatch {case}"
        );
    }
    let mut evidence = scene.clone();
    evidence.objects[0].template = Some(TemplateEvidence {
        api_version: "kagami.dev/v2".into(),
        fingerprint: ArtifactDigest::sha256_of(b"absent historical source is NOT a required blob"),
    });
    evidence.objects[0]
        .components
        .iter_mut()
        .flat_map(|c| &mut c.properties)
        .find(|p| p.id.as_str() == "source")
        .unwrap()
        .source
        .as_mut()
        .unwrap()
        .expression = "deliberately non-evaluable historical evidence".into();
    let accepted = recompile(&evidence, compiled.projection().definition().clone()).unwrap();
    assert_ne!(
        accepted.verified().root(),
        compiled.compiled().verified().root()
    );
    assert_eq!(accepted.verified().scene().unwrap(), &evidence);
    assert!(
        !accepted
            .verified()
            .manifest()
            .spec
            .artifacts
            .iter()
            .any(|a| a.digest == evidence.objects[0].template.as_ref().unwrap().fingerprint)
    );
}

fn assert_retention_admission(
    document: &kagami_session::ExperimentDocument,
    schemas: &kagami_catalog::SchemaRegistry,
) {
    use kagami_document::scientific::{ScientificError, ScientificRetention};
    use kagami_document::{ExperimentCommand as Edit, Limits, Setup, TimeStep};
    use kagami_session::container::{self, ContainerLimits};
    use kagami_session::{
        ActorId, CommandId, DocumentAuthority, ExperimentCommandEnvelope, SessionCommand,
    };
    let envelope = |id: &str, command| {
        ExperimentCommandEnvelope::new(
            CommandId::new(id).unwrap(),
            ActorId::new("test").unwrap(),
            command,
        )
    };
    let Setup::Scientific(shared) = &document.experiment.setup else {
        panic!("scientific test fixture")
    };
    let mut tally = ScientificRetention::new(usize::MAX);
    tally.include(shared).unwrap();
    let one = tally.bytes();
    assert!(one > 0);
    tally.include(shared).unwrap();
    assert_eq!(tally.bytes(), one);
    let mut refused = ScientificRetention::new(one - 1);
    assert_eq!(refused.include(shared), Err(ScientificError::Retention));
    assert_eq!(refused.include(shared), Err(ScientificError::Retention));
    let encoded = container::encode(document, ContainerLimits::default()).unwrap();
    let fresh = container::decode(&encoded, ContainerLimits::default()).unwrap();
    let Setup::Scientific(independent) = &fresh.experiment.setup else {
        unreachable!()
    };
    tally.include(independent).unwrap();
    // Identical digests in two allocations are two physical buffers, not a free
    // cache hit. Metadata belongs to separate capture ownership too.
    assert_eq!(tally.bytes(), one * 2);
    let mut limits = Limits::default();
    limits.scientific.retained_bytes = one;
    let open = |document: kagami_session::ExperimentDocument| SessionCommand::Open {
        experiment: Box::new(
            document
                .into_experiment(schemas, &Limits::default())
                .unwrap(),
        ),
        target: None,
        discard_unsaved: true,
    };
    let mut authority = DocumentAuthority::new(schemas.clone(), limits);
    let first = envelope("open-first", open(document.clone())).guarded_by(authority.revision());
    authority.submit(first.clone()).unwrap();
    assert_eq!(authority.scientific_retained_bytes().unwrap(), one);
    authority
        .submit(envelope("open-shared", open(document.clone())))
        .unwrap();
    assert_eq!(authority.scientific_retained_bytes().unwrap(), one);
    authority
        .submit(envelope(
            "dt",
            SessionCommand::Edit(vec![Edit::SetTimeStep(TimeStep::new(0.002).unwrap())]),
        ))
        .unwrap();
    authority
        .submit(envelope("undo", SessionCommand::Undo))
        .unwrap();
    authority
        .submit(envelope("redo", SessionCommand::Redo))
        .unwrap();
    assert_eq!(authority.scientific_retained_bytes().unwrap(), one);

    let gesture = authority
        .submit(envelope("gesture", SessionCommand::BeginInteractiveEdit))
        .unwrap()
        .change;
    let kagami_session::ExperimentChange::GestureOpened { gesture } = gesture else {
        unreachable!()
    };
    let snapshot = authority.snapshot();
    let history = authority.history_status();
    let event_count = authority
        .events_since(kagami_session::EventSeq::INITIAL)
        .count();
    let oversized = envelope(
        "new-capture",
        SessionCommand::Edit(vec![Edit::AdoptScientificSetup(independent.clone())]),
    );
    assert_eq!(
        authority.preflight(&oversized).unwrap_err().code(),
        "scientific_retention_limit"
    );
    assert_eq!(
        authority.submit(oversized).unwrap_err().code(),
        "scientific_retention_limit"
    );
    assert_eq!(authority.snapshot(), snapshot);
    assert_eq!(authority.history_status(), history);
    assert_eq!(
        authority
            .events_since(kagami_session::EventSeq::INITIAL)
            .count(),
        event_count
    );
    assert_eq!(authority.scientific_retained_bytes().unwrap(), one);
    // Rejected capture does not implicitly close a currently open gesture.
    assert!(authority.submit(first.clone()).unwrap().replayed);
    authority
        .submit(envelope("end", SessionCommand::EndInteractiveEdit).within(gesture))
        .unwrap();
    let intermediate = envelope(
        "intermediate",
        SessionCommand::Edit(vec![
            Edit::AdoptScientificSetup(independent.clone()),
            Edit::AdoptScientificSetup(shared.clone()),
        ]),
    );
    assert_eq!(
        authority.submit(intermediate).unwrap_err().code(),
        "scientific_retention_limit"
    );
    assert_eq!(authority.snapshot(), snapshot);

    // Replacement legitimately discards undo/redo. Old *replay* receipts may be
    // evicted to fit, but the newly accepted receipt must remain replayable.
    let second = envelope("open-independent", open(fresh)).guarded_by(authority.revision());
    let accepted = authority.submit(second.clone()).unwrap();
    assert!(!accepted.replayed);
    assert_eq!(authority.scientific_retained_bytes().unwrap(), one);
    assert!(authority.history_status().undo.is_none());
    assert!(authority.submit(second).unwrap().replayed);
    assert_eq!(
        authority.submit(first).unwrap_err().code(),
        "revision_conflict"
    );

    limits.scientific.retained_bytes = one - 1;
    let mut too_small = DocumentAuthority::new(schemas.clone(), limits);
    let revision = too_small.revision();
    assert_eq!(
        too_small
            .submit(envelope("open", open(document.clone())))
            .unwrap_err()
            .code(),
        "scientific_retention_limit"
    );
    assert_eq!(too_small.revision(), revision);
    assert!(!too_small.is_dirty());
    limits.scientific.retained_bytes = one;
    limits.scientific.blob_bytes = 1;
    let mut too_small = DocumentAuthority::new(schemas.clone(), limits);
    assert_eq!(
        too_small
            .submit(envelope("open", open(document.clone())))
            .unwrap_err()
            .code(),
        "scientific_setup_limit"
    );
}
