use kagami::document::Document;
use kagami_document::{DisplayName, ExperimentCommand, ObjectSpec};
use kagami_session::{Projection, RunAttachment, RunLabel, SessionCommand};

fn edit() -> Vec<ExperimentCommand> {
    vec![ExperimentCommand::CreateObject(Box::new(ObjectSpec::new(
        DisplayName::new("candidate").unwrap(),
    )))]
}

#[test]
fn effects_cannot_cross_documents_replacements_revisions_schemas_or_modes() {
    let mut document = Document::new(Default::default(), Default::default());
    let guard = document.authoring_guard().unwrap();
    let mut other = Document::new(Default::default(), Default::default());
    assert_eq!(document.view().revision(), other.view().revision());
    assert!(!other.edit_guarded(guard, edit()));
    assert_eq!(other.snapshot().object_count(), 0);
    assert!(document.new_experiment(true));
    assert!(!document.edit_guarded(guard, edit()));
    let guard = document.authoring_guard().unwrap();
    assert!(document.edit(edit()));
    assert!(!document.edit_guarded(guard, edit()));
    assert!(document.submit(SessionCommand::Undo));
    assert!(
        !document.edit_guarded(guard, edit()),
        "undo cannot resurrect a stale effect"
    );
    let guard = document.authoring_guard().unwrap();
    document.adopt_schemas(Default::default());
    assert!(!document.edit_guarded(guard, edit()));
    let guard = document.authoring_guard().unwrap();
    document
        .observe(RunAttachment::remote(RunLabel::new("run").unwrap()))
        .unwrap();
    assert!(document.authoring_guard().is_none());
    assert!(!document.edit_guarded(guard, edit()));
    document.edit_initial_conditions().unwrap();
    assert!(
        !document.edit_guarded(guard, edit()),
        "leaving observation is not permission for an old effect"
    );
    assert_eq!(document.snapshot().object_count(), 0);
}

#[test]
fn view_changes_allow_one_guarded_undoable_adoption_but_not_duplicate_delivery() {
    let mut document = Document::new(Default::default(), Default::default());
    let guard = document.authoring_guard().unwrap();
    document.set_projection(Projection::Orthographic);
    assert!(document.accepts_effect(guard));
    assert!(document.edit_guarded(guard, edit()));
    let revision = document.view().revision();
    assert_eq!(document.snapshot().object_count(), 1);
    assert!(!document.edit_guarded(guard, edit()));
    assert_eq!(document.view().revision(), revision);
    assert!(document.submit(SessionCommand::Undo));
    assert_eq!(document.snapshot().object_count(), 0);
    assert!(document.submit(SessionCommand::Redo));
    assert_eq!(document.snapshot().object_count(), 1);
}

#[test]
fn effect_variable_context_preserves_dimensions_and_honors_tighter_source_limits() {
    use kagami_document::{Limits, VariableSpec, variable_context};
    let mut document = Document::new(Default::default(), Default::default());
    assert!(
        document.edit(vec![ExperimentCommand::DefineVariable(Box::new(
            VariableSpec::new(orishu_variables::Name::new("mass").unwrap(), "2000 g")
        ))])
    );
    let variables = variable_context(document.snapshot(), document.limits()).unwrap();
    let value = variables.eval("mass / 2").unwrap();
    assert_eq!(value.magnitude(), 1.0);
    assert_eq!(value.dimension(), orishu_variables::Dimension::MASS);
    assert!(
        variable_context(
            document.snapshot(),
            &Limits {
                max_expression_bytes: 2,
                ..Limits::DEFAULT
            }
        )
        .is_err()
    );
    assert!(
        variable_context(
            document.snapshot(),
            &Limits {
                max_variables: 0,
                ..Limits::DEFAULT
            }
        )
        .is_err()
    );
}
