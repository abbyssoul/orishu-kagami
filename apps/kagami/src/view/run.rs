use crate::{
    message::{Message, WorkspaceIntent},
    model::Model,
    run::Action,
};
use iced::{
    Element, Length,
    widget::{button, column, container, row, scrollable, text, tooltip},
};
mod fields;

/// A run action. A disabled action explains itself in a tooltip.
fn control<'a>(
    label: &'a str,
    action: Action,
    availability: Result<(), &'a str>,
) -> Element<'a, Message> {
    available(button(text(label)), Message::Run(action), availability)
}
fn available<'a>(
    button: iced::widget::Button<'a, Message>,
    message: Message,
    availability: Result<(), &'a str>,
) -> Element<'a, Message> {
    match availability {
        Ok(()) => button.on_press(message).into(),
        Err(reason) => tooltip(
            button,
            container(text(reason).size(12))
                .padding(6)
                .max_width(320)
                .style(container::rounded_box),
            tooltip::Position::Bottom,
        )
        .into(),
    }
}
pub(super) fn panel(model: &Model) -> Element<'_, Message> {
    let run = &model.run;
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
    } else if let Some(reason) = run.recording_blocked() {
        content = content.push(
            text(format!(
                "Submissions and run commands are disabled; observation stays available. {reason}"
            ))
            .size(12),
        );
    }
    content = content.push(control(
        "Inspect retained run",
        Action::Discover,
        run.check(&Action::Discover),
    ));
    if let Some(status) = run.status() {
        let identity = status.descriptor().identity();
        content = content.push(text(format!("Formation: {}\nWorkload: {}\nEpoch: {}\nLast confirmed boundary: {}\nSimulation time: {} s\nPhase: {:?}", identity.formation_id(), identity.workload_id(), identity.workload_epoch().get(), status.boundary(), status.time_seconds(), status.phase())).size(12));
        if !run.attached() {
            content = content.push(control(
                "Observe this exact run",
                Action::Observe,
                if model.is_authoring() {
                    run.check(&Action::Observe)
                } else {
                    Err("Return to the authoring document first.")
                },
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
            content = content.push(control("Reconcile original load", Action::ReconcileLoad, run.check(&Action::ReconcileLoad)))
                .push(control("Resubmit identical frozen workload", Action::ResubmitLoad, run.check(&Action::ResubmitLoad)))
                .push(text(format!("Recorded with its frozen bytes for worker {}. After a restart, Kagami restores it for explicit reconciliation. A lost reply is not a refusal.", load.target.address())).size(11));
        } else if !run.attached() {
            if load.accepted().is_some() {
                content = content.push(control(
                    "Inspect accepted run",
                    Action::InspectLoaded,
                    run.check(&Action::InspectLoaded),
                ));
            }
            content = content.push(control(
                "Clear final submission history",
                Action::ClearLoad,
                run.check(&Action::ClearLoad),
            ));
        }
    }
    if run.attached() {
        content = content
            .push(
                row![
                    control("3D positions", Action::NumericView(false), Ok(())),
                    control("Numeric table", Action::NumericView(true), Ok(()))
                ]
                .spacing(6),
            )
            .push(text("Scene scale (m per render unit); observation-only:").size(11))
            .push(iced::widget::pick_list(
                kagami_session::SceneScale::PRESETS,
                Some(model.current_view().scale()),
                |s| crate::message::ClientLocal::SetScale(s).into(),
            ));
        content = content.push(control("Refresh committed values", Action::Refresh, run.check(&Action::Refresh)))
            .push(row![control("Step once", Action::Step, run.check(&Action::Step)), control("Finish run", Action::Finish, run.check(&Action::Finish))].spacing(6))
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
                run.check(&Action::Reconcile),
            ))
            .push(control("Resubmit identical original intent", Action::ResubmitOriginal, run.check(&Action::ResubmitOriginal)))
            .push(text("This command is recorded. After a restart, Kagami restores it for explicit reconciliation. Resubmission keeps its original identity and precondition.").size(11));
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
        content = content.push(control("Close panel", Action::Close, Ok(())));
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
        available(
            button(text("Prepare captured revision")),
            Message::Workload(W::Prepare),
            if !p.configured() {
                Err("No plugin inventory is open, so no workload can be prepared.")
            } else if p.is_pending() || model.run.is_pending() {
                Err("Another request is in progress. Wait until it completes.")
            } else if model.run.submission().is_some_and(|s| s.unresolved()) {
                Err("Reconcile the recorded submission first.")
            } else if model.run.submission().is_some() {
                Err("Clear the final submission history before you prepare another workload.")
            } else {
                Ok(())
            },
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
            .push(available(
                button(text("Submit frozen workload")),
                Message::Workload(W::Submit),
                if p.is_pending() {
                    Err("Another request is in progress. Wait until it completes.")
                } else if p.formation.parse::<orishu::model::cluster::FormationId>().is_err() {
                    Err("Enter the exact target formation ID.")
                } else {
                    model.run.check_submit()
                },
            ));
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
