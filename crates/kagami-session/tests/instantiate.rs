//! Catalog instantiation, exercised through the authority.
//!
//! K5's acceptance cases. The property that matters most is a negative one:
//! ADR 0008 says instantiation *materialises* rather than links, so an object
//! that came from a template must be completely indifferent to what happens to
//! that template afterwards. That is asserted here rather than asserted in a
//! comment, because the failure mode — a tracking link creeping back in —
//! would otherwise be invisible until something propagated.

mod support;

use kagami_catalog::{
    CatalogSet, ContentFingerprint, ParameterName, PropertyName, TemplateIdentity, load_directory,
};
use kagami_document::{DisplayName, Limits};
use kagami_session::{
    ActorId, CommandId, DocumentAuthority, ExperimentCommandEnvelope, InstantiationSpec,
    SessionCommand,
};
use support::schemas;

/// A catalog directory holding one template that reads its own parameter.
fn catalog_source() -> &'static str {
    r#"
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata:
  catalog: planets
  name: sun
spec:
  parameters:
    solar_mass: {default: "1.989e30", unit: kg}
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: "planets.sun.solar_mass"}
"#
}

/// Write `source` into a fresh directory and load it as a catalog snapshot.
fn catalog_from(source: &str) -> (tempdir::TempDir, CatalogSet) {
    let directory = tempdir::TempDir::new();
    std::fs::write(directory.path().join("planets.yaml"), source).expect("fixture is writable");
    let set = load_directory(
        directory.path(),
        &schemas(),
        &kagami_catalog::Limits::DEFAULT,
    );
    (directory, set)
}

fn catalog() -> (tempdir::TempDir, CatalogSet) {
    catalog_from(catalog_source())
}

fn template() -> TemplateIdentity {
    TemplateIdentity::new(
        "planets".try_into().expect("valid catalog name"),
        "sun".try_into().expect("valid template name"),
    )
}

fn label(value: &str) -> DisplayName {
    DisplayName::new(value).expect("valid label")
}

fn property(name: &str) -> PropertyName {
    PropertyName::new(name).expect("valid identifier")
}

fn authority_with_catalog() -> (tempdir::TempDir, DocumentAuthority) {
    let (directory, set) = catalog();
    let mut authority = DocumentAuthority::new(schemas(), Limits::DEFAULT);
    authority.adopt_catalog(set);
    (directory, authority)
}

fn instantiate(
    authority: &mut DocumentAuthority,
    id: &str,
    spec: InstantiationSpec,
) -> Result<kagami_session::Acceptance, kagami_session::SessionRejection> {
    authority.submit(ExperimentCommandEnvelope::new(
        CommandId::new(id).expect("valid"),
        ActorId::new("ui").expect("valid"),
        SessionCommand::InstantiateObjectTemplate(Box::new(spec)),
    ))
}

/// The stored mass of the only object in the experiment.
fn only_mass(authority: &DocumentAuthority) -> f64 {
    let snapshot = authority.snapshot();
    let object = snapshot.objects().values().next().expect("one object");
    object
        .components
        .values()
        .next()
        .expect("one component")
        .properties[&property("mass")]
        .si_value()
        .expect("a priced quantity")
}

#[test]
fn instantiating_a_template_produces_one_revision_and_one_undo_entry() {
    let (_directory, mut authority) = authority_with_catalog();
    let before = authority.revision();

    let accepted = instantiate(
        &mut authority,
        "ui-1",
        InstantiationSpec::new(template(), label("Sun")),
    )
    .expect("an available template");

    assert_eq!(accepted.revision, before.next());
    assert_eq!(authority.snapshot().object_count(), 1);
    assert_eq!(authority.history_status().depth, 1);
    assert_eq!(only_mass(&authority), 1.989e30);
}

#[test]
fn the_object_resolves_from_its_own_copied_definitions() {
    let (_directory, mut authority) = authority_with_catalog();
    instantiate(
        &mut authority,
        "ui-1",
        InstantiationSpec::new(template(), label("Sun")),
    )
    .expect("accepted");

    // The definitions came with it, in the object's own scope.
    let snapshot = authority.snapshot();
    assert_eq!(snapshot.variable_count(), 1);
    let definition = snapshot.variables().values().next().expect("one copied");
    assert!(
        definition.namespace.as_str().starts_with("objects.object_"),
        "definitions are copied into the object's own scope: {}",
        definition.namespace.as_str()
    );

    // And every magnitude was re-derived by the document, not taken on trust
    // from the catalog: the value the model stores resolves from what it
    // stores.
    let values = kagami_document::resolve_variables(&snapshot, &Limits::DEFAULT)
        .expect("the copied closure resolves on its own");
    assert_eq!(values.len(), 1);
    assert_eq!(values.values().next(), Some(&1.989e30));
}

#[test]
fn the_object_records_where_it_came_from_as_evidence() {
    let (_directory, mut authority) = authority_with_catalog();
    instantiate(
        &mut authority,
        "ui-1",
        InstantiationSpec::new(template(), label("Sun")),
    )
    .expect("accepted");

    let snapshot = authority.snapshot();
    let object = snapshot.objects().values().next().expect("one object");
    let provenance = object
        .provenance
        .as_ref()
        .expect("materialised objects say so");
    assert_eq!(provenance.identity, template());

    // A hand-authored object carries none, so a projection can tell them
    // apart without consulting the catalog.
    let mut agent = support::Adapter::unguarded("agent");
    agent.edit(&mut authority, vec![support::create("By hand")]);
    let snapshot = authority.snapshot();
    let hand_authored = snapshot
        .objects()
        .values()
        .find(|object| object.name.as_str() == "By hand")
        .expect("created");
    assert_eq!(hand_authored.provenance, None);
}

#[test]
fn changing_the_template_afterwards_leaves_the_object_identical() {
    let (_directory, mut authority) = authority_with_catalog();
    instantiate(
        &mut authority,
        "ui-1",
        InstantiationSpec::new(template(), label("Sun")),
    )
    .expect("accepted");
    let before = authority.snapshot().objects().clone();
    let revision = authority.revision();

    // The template is edited to a completely different mass, and reloaded.
    let (_edited_directory, edited) = catalog_from(&catalog_source().replace("1.989e30", "42"));
    authority.adopt_catalog(edited);

    assert_eq!(authority.snapshot().objects(), &before);
    assert_eq!(only_mass(&authority), 1.989e30);
    assert_eq!(authority.revision(), revision, "nothing propagated");

    // And the template being deleted outright changes nothing either. There
    // is no propagation path to invoke — that is the point of ADR 0008, and
    // this is the test that would fail if a tracking link came back.
    authority.adopt_catalog(CatalogSet::default());
    assert_eq!(authority.snapshot().objects(), &before);
    assert_eq!(only_mass(&authority), 1.989e30);
    assert_eq!(authority.revision(), revision);
}

#[test]
fn a_parameter_override_is_an_authored_expression() {
    let (_directory, mut authority) = authority_with_catalog();
    instantiate(
        &mut authority,
        "ui-1",
        InstantiationSpec::new(template(), label("Half a sun")).binding(
            ParameterName::new("solar_mass").expect("valid identifier"),
            "1.989e30 / 2",
        ),
    )
    .expect("accepted");

    assert_eq!(only_mass(&authority), 1.989e30 / 2.0);
}

#[test]
fn a_stale_fingerprint_is_refused_rather_than_silently_upgraded() {
    let (_directory, mut authority) = authority_with_catalog();

    // The caller believes it is instantiating content that is not there.
    let stale = ContentFingerprint::of(b"something else");
    let refused = instantiate(
        &mut authority,
        "ui-1",
        InstantiationSpec::new(template(), label("Sun")).expecting(stale),
    )
    .expect_err("the caller picked content that has since changed");

    assert_eq!(refused.code(), "instantiation_refused");
    assert_eq!(authority.snapshot().object_count(), 0);
    assert_eq!(authority.snapshot().variable_count(), 0);
}

#[test]
fn an_unknown_template_is_refused_with_the_catalogs_own_reason() {
    let (_directory, mut authority) = authority_with_catalog();
    let missing = TemplateIdentity::new(
        "planets".try_into().expect("valid"),
        "pluto".try_into().expect("valid"),
    );
    let refused = instantiate(
        &mut authority,
        "ui-1",
        InstantiationSpec::new(missing, label("Pluto")),
    )
    .expect_err("no such template");

    assert_eq!(refused.code(), "instantiation_refused");
    assert!(
        refused.to_string().contains("planets/pluto"),
        "the catalog's own reason names what was missing: {refused}"
    );
    // Nothing partial: not the object, not its definitions.
    assert_eq!(authority.snapshot().object_count(), 0);
    assert_eq!(authority.snapshot().variable_count(), 0);
    assert_eq!(authority.revision().get(), 0);
}

#[test]
fn instantiating_without_a_catalog_is_refused() {
    let mut authority = DocumentAuthority::new(schemas(), Limits::DEFAULT);
    let refused = instantiate(
        &mut authority,
        "ui-1",
        InstantiationSpec::new(template(), label("Sun")),
    )
    .expect_err("no catalog is loaded");
    assert_eq!(refused.code(), "no_catalog_loaded");
}

#[test]
fn an_instantiation_undoes_as_one_step() {
    let (_directory, mut authority) = authority_with_catalog();
    instantiate(
        &mut authority,
        "ui-1",
        InstantiationSpec::new(template(), label("Sun")),
    )
    .expect("accepted");
    assert_eq!(authority.snapshot().object_count(), 1);
    assert_eq!(authority.snapshot().variable_count(), 1);

    authority
        .submit(ExperimentCommandEnvelope::new(
            CommandId::new("ui-2").expect("valid"),
            ActorId::new("ui").expect("valid"),
            SessionCommand::Undo,
        ))
        .expect("one entry");

    // The object *and* its copied definitions go together: they arrived as
    // one edit, so they leave as one.
    assert_eq!(authority.snapshot().object_count(), 0);
    assert_eq!(authority.snapshot().variable_count(), 0);
}

#[test]
fn two_instantiations_do_not_collide() {
    let (_directory, mut authority) = authority_with_catalog();
    instantiate(
        &mut authority,
        "ui-1",
        InstantiationSpec::new(template(), label("Sun")),
    )
    .expect("accepted");
    instantiate(
        &mut authority,
        "ui-2",
        InstantiationSpec::new(template(), label("Another sun")),
    )
    .expect("accepted");

    // Each object's definitions live in its own scope, so the second copy of
    // the same template does not clash with the first.
    let snapshot = authority.snapshot();
    assert_eq!(snapshot.object_count(), 2);
    assert_eq!(snapshot.variable_count(), 2);
    let scopes: std::collections::BTreeSet<_> = snapshot
        .variables()
        .values()
        .map(|definition| definition.namespace.as_str().to_owned())
        .collect();
    assert_eq!(
        scopes.len(),
        2,
        "each object owns its own scope: {scopes:?}"
    );
}

/// A directory that deletes itself, so a catalog fixture leaves nothing behind.
mod tempdir {
    use std::path::{Path, PathBuf};

    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "kagami-instantiate-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a temporary directory is creatable");
            Self(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
