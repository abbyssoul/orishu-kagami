use crate::{
    message::{Message, WorkspaceIntent},
    model::Model,
    run::Action,
};
use iced::{
    Element, Length,
    widget::{button, column, container, row, scrollable, text},
};
mod fields;

fn control(label: &'static str, action: Action, enabled: bool) -> Element<'static, Message> {
    button(text(label))
        .on_press_maybe(enabled.then_some(Message::Run(action)))
        .into()
}
pub(super) fn panel(model: &Model) -> Element<'_, Message> {
    let run = &model.run;
    let ready = run.configured() && !run.is_pending();
    let lineage = run.source_revision().map(|revision| format!("Submitted from this session's captured revision {revision}. The current draft may have changed since submission."))
        .unwrap_or_else(|| "External or unassociated run. The open draft is not claimed as its initial conditions.".into());
    let mut content = column![
        text("Remote run").size(18),
        text(lineage).size(12),
        text(format!("Worker: {}", model.cluster_address)).size(12),
        text(&run.notice).size(12),
    ]
    .spacing(8);
    #[cfg(unix)]
    if model.is_authoring() {
        content = content.push(preparation(model));
    }
    if !run.configured() {
        content = content.push(text("Enable access at launch with --operator-token-file (and --ca-cert for extra TLS trust).").size(12));
    }
    content = content.push(control(
        "Inspect retained run",
        Action::Discover,
        ready
            && !run.attached()
            && run.command().is_none()
            && !run.submission().is_some_and(|s| s.unresolved()),
    ));
    if let Some(status) = run.status() {
        let identity = status.descriptor().identity();
        content = content.push(text(format!("Formation: {}\nWorkload: {}\nEpoch: {}\nLast confirmed boundary: {}\nSimulation time: {} s\nPhase: {:?}", identity.formation_id(), identity.workload_id(), identity.workload_epoch().get(), status.boundary(), status.time_seconds(), status.phase())).size(12));
        if !run.attached() {
            content = content.push(control(
                "Observe this exact run",
                Action::Observe,
                ready
                    && model.is_authoring()
                    && run.command().is_none()
                    && !run.submission().is_some_and(|s| s.unresolved()),
            ));
        }
    }
    if let Some(load) = run.submission() {
        content = content.push(
            text(format!(
                "Submission: {}\nFormation: {}\nWorkload: {}\nSource context: {}\nCaptured revision: {}\nReceipt: {:?}",
                String::from(load.request.operation_id().clone()),
                load.request.formation_id(),
                load.request.workload_id(),
                load.source_context,
                load.source_revision,
                load.receipt.as_ref().map(|r| r.state())
            ))
            .size(11),
        );
        if load.unresolved() {
            content = content.push(control("Reconcile original load", Action::ReconcileLoad, ready))
                .push(control("Resubmit identical frozen workload", Action::ResubmitLoad, ready))
                .push(text("Load intent and frozen bytes are currently memory-only. Record the exact formation/operation/workload before closing. A lost reply is not a refusal.").size(11));
        } else if !run.attached() {
            if load.accepted().is_some() {
                content = content.push(control(
                    "Inspect accepted run",
                    Action::InspectLoaded,
                    ready,
                ));
            }
            content = content.push(control(
                "Clear final submission history",
                Action::ClearLoad,
                ready,
            ));
        }
    }
    if run.attached() {
        content = content
            .push(
                row![
                    control("3D positions", Action::NumericView(false), true),
                    control("Numeric table", Action::NumericView(true), true)
                ]
                .spacing(6),
            )
            .push(text("Scene scale (m per render unit); observation-only:").size(11))
            .push(iced::widget::pick_list(
                kagami_session::SceneScale::PRESETS,
                Some(model.current_view().scale()),
                |s| crate::message::ClientLocal::SetScale(s).into(),
            ));
        content = content.push(control("Refresh committed values", Action::Refresh, ready))
            .push(row![control("Step once", Action::Step, run.can_control()), control("Finish run", Action::Finish, run.can_control())].spacing(6))
            .push(text("Finish is terminal, not pause. These controls change the remote run for every observer.").size(12))
            .push(button(text("Return to open authoring document")).on_press(WorkspaceIntent::EditInitialConditions.into()))
            .push(fields::view(model));
    }
    if let Some(intent) = run.command() {
        content = content
            .push(
                text(format!(
                    "Unresolved {:?}: {}\nExpected boundary: {}",
                    intent.command(),
                    String::from(intent.operation_id().clone()),
                    intent.expected_boundary()
                ))
                .size(12),
            )
            .push(control(
                "Reconcile original command",
                Action::Reconcile,
                ready,
            ))
            .push(control("Resubmit identical original intent", Action::ResubmitOriginal, ready))
            .push(text("Record this intent before closing: client reconciliation state is currently in-memory only. Resubmission keeps its original identity and precondition.").size(11));
    }
    if let Some(receipt) = run.receipt() {
        content = content.push(
            text(format!(
                "Historical command receipt:\n{}\n{:?}",
                String::from(receipt.request().operation_id().clone()),
                receipt.state()
            ))
            .size(11),
        );
    }
    if !run.attached() {
        content = content.push(control("Close panel", Action::Close, true));
    }
    container(scrollable(content))
        .padding(10)
        .width(Length::Fixed(360.0))
        .height(Length::Fill)
        .into()
}

#[cfg(unix)]
fn preparation(model: &Model) -> Element<'_, Message> {
    use crate::workload_preparation::Action as W;
    use iced::widget::text_input;
    let p = &model.workload_preparation;
    let frozen = p.ready(&model.document);
    let mut content = column![
        text("Prepare the open experiment").size(15),
        text_input("Workload name", &p.name).on_input(|v| Message::Workload(W::Name(v))),
        button(text("Prepare captured revision")).on_press_maybe(
            (p.configured()
                && !p.is_pending()
                && !model.run.is_pending()
                && model.run.submission().is_none())
            .then_some(Message::Workload(W::Prepare))
        ),
        text(&p.notice).size(12),
    ]
    .spacing(6);
    if p.can_cancel() {
        content =
            content.push(button(text("Cancel preparation")).on_press(Message::Workload(W::Cancel)));
    }
    if let Some(frozen) = frozen {
        content = content.push(text(format!("Revision {} → {}\n{} bytes; {} artifacts; inventory revision {}", frozen.source_revision, frozen.report.workload, frozen.report.bytes, frozen.report.artifacts, frozen.report.inventory_revision)).size(11))
            .push(button(text("Export frozen workload…")).on_press_maybe((!p.is_pending()).then_some(Message::Workload(W::Export))))
            .push(text_input("Exact target formation ID", &p.formation).on_input(|v| Message::Workload(W::Formation(v))))
            .push(text("Submit sends this exact workload. The worker must already be locked and scientifically enabled; Kagami never locks it automatically.").size(11))
            .push(button(text("Submit frozen workload")).on_press_maybe((!p.is_pending() && model.run.can_submit() && p.formation.parse::<orishu::model::cluster::FormationId>().is_ok()).then_some(Message::Workload(W::Submit))));
    }
    container(content).into()
}

pub(super) fn numeric_view(model: &Model) -> Element<'_, Message> {
    let Some(frame) = model.run.objects() else {
        return container(text("No current numeric projection. Refresh committed values.\nA command receipt is not an observation.")).padding(16).width(Length::Fill).height(Length::Fill).into();
    };
    let mut content = column![text("Committed numeric objects").size(18),
        text(format!("Showing {} of {} objects; display truncation only. Forces evaluated at {:?} (none = initial/uncomputed).", frame.rows.len(), frame.count, frame.force_boundary)).size(12),
        text(format!("{:?}", frame.source)).size(11),
    ].spacing(10);
    let vector = |v: [orishu_plugin::FiniteF64; 3]| {
        format!(
            "[{:.6e}, {:.6e}, {:.6e}]",
            v[0].get(),
            v[1].get(),
            v[2].get()
        )
    };
    for row in &frame.rows {
        let object = &row.object;
        content = content.push(
            text(format!(
                "Object {} — {}\nPosition (m): {}\nVelocity (m/s): {}\nForce (N): {}",
                object.id.0,
                if object.inertial_mass_kilograms.is_some() {
                    "Dynamics"
                } else {
                    "static/kinematic"
                },
                vector(object.kinematics.position_metres),
                vector(object.kinematics.velocity_metres_per_second),
                row.force
                    .map(|f| vector(f.newtons))
                    .unwrap_or_else(|| "not computed / no Dynamics".into())
            ))
            .size(13),
        );
    }
    container(scrollable(content))
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub(super) fn observation_view(model: &Model) -> Element<'_, Message> {
    if model.run.numeric_view || model.run.objects().is_none() {
        return numeric_view(model);
    }
    let frame = model.run.objects().expect("frame checked");
    let geometry = &frame.geometry;
    let description = if geometry.scale == model.current_view().scale() {
        format!(
            "{} of {} positions; {} outside scale/precision range, {} beyond marker budget. Camera clipping also applies.",
            geometry.markers.markers().len(),
            frame.count,
            geometry.omitted_scale,
            geometry.omitted_capacity
        )
    } else {
        "Reprojecting at the new scene scale; old-scale markers are hidden.".into()
    };
    column![
        text("Committed object positions — manual snapshot").size(17),
        text("Gold: Dynamics. Blue: static/kinematic. Fixed-size markers are not physical radii.")
            .size(12),
        text(description).size(12),
        text(format!("{:?}", frame.source)).size(10),
        crate::viewport::view(model),
    ]
    .spacing(5)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
