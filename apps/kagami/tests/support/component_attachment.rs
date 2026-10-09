//! Drive the actual two-phase native attachment workflow for unambiguous fixtures.
//! Tests about ambiguity/invalidity use the individual actions instead.
use kagami::{
    component_form, message::Message, model::Model, physics_form::dependencies::Action,
    update::update,
};
use std::time::{Duration, Instant};

fn drive(model: &mut Model, action: Action) {
    let _ = update(
        model,
        Message::ComponentForm(component_form::Action::Dependencies(action)),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while model.component_dependencies.is_pending() || model.plugin_references.is_pending() {
        assert!(
            Instant::now() < deadline,
            "{}",
            model.component_dependencies.notice
        );
        std::thread::sleep(Duration::from_millis(1));
        let _ = update(
            model,
            Message::ComponentForm(component_form::Action::Dependencies(Action::Poll)),
        );
    }
}

pub fn accept(model: &mut Model) {
    assert!(model.component_form.attachment().is_some());
    assert!(model.component_form.current(&model.document));
    drive(model, Action::Check);
    let report = model
        .component_dependencies
        .report()
        .expect(&model.component_dependencies.notice);
    assert!(
        matches!(
            report.outcome(),
            orishu_plugin::resolution::ResolutionOutcome::Resolved { .. }
        ),
        "{:?}",
        report.outcome()
    );
    let token = report.token();
    drive(model, Action::ApplyLock { report: token });
    assert!(
        model.document.notice.is_none(),
        "{:?}; {}",
        model.document.notice,
        model.component_dependencies.notice
    );
    assert!(
        !model.component_form.current(&model.document),
        "adoption must advance the document"
    );
}
