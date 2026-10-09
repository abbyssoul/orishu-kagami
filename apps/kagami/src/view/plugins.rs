use crate::{
    message::Message,
    model::Model,
    plugins::{InventoryCommand, window::Action},
};
use iced::{
    Element, Length,
    widget::{button, column, container, row, scrollable, text},
};

pub(super) fn panel(model: &Model) -> Element<'_, Message> {
    let manager = &model.plugin_management;
    let ready = !manager.is_pending();
    let control = |label: &'static str, action, enabled: bool| {
        button(text(label)).on_press_maybe(enabled.then_some(Message::Plugins(action)))
    };
    let mut content = column![
        text("Local plugins").size(18),
        text("Inventory preferences only: installed updates never migrate exact experiment pins or change accepted runs.").size(12),
        text("Refresh adopts current availability and clears unapplied physics-form choices/prepared candidates when the revision changes.").size(12),
        text(&manager.notice).size(12),
        text(&model.plugin_references.notice).size(12),
        control("Recheck reference leases", Action::RetryReferences, !model.plugin_references.is_pending()),
        row![control("Refresh", Action::Refresh, ready), control("Install bundle…", Action::PickBundle(None), ready && manager.listing.is_some())].spacing(6),
        control("Close", Action::Close, true),
    ].spacing(8);
    for (plugin, enabled) in manager.overrides() {
        content = content.push(
            text(format!(
                "This run overrides {plugin}: {} (persistent preference below is separate)",
                if *enabled { "enabled" } else { "disabled" }
            ))
            .size(11),
        );
    }
    if let Some(listing) = &manager.listing {
        content = content.push(text(format!(
            "Inventory revision {} — {} releases",
            listing.revision,
            listing.releases.len()
        )));
        for entry in &listing.releases {
            content = content.push(
                text(format!(
                    "{}\n{}\n{} · {}",
                    entry.plugin_id,
                    entry.release,
                    if entry.enabled {
                        "enabled preference"
                    } else {
                        "disabled preference"
                    },
                    if entry.is_default {
                        "default"
                    } else {
                        "side-by-side"
                    }
                ))
                .size(12),
            );
            content = content.push(
                row![
                    control("Inspect", Action::Inspect(entry.release), ready),
                    control("References", Action::References(entry.release), ready),
                    control(
                        if entry.enabled { "Disable" } else { "Enable" },
                        Action::Mutate(InventoryCommand::SetEnabled {
                            plugin_id: entry.plugin_id.clone(),
                            enabled: !entry.enabled
                        }),
                        ready
                    ),
                    control(
                        "Set default",
                        Action::Mutate(InventoryCommand::SetDefault {
                            plugin_id: entry.plugin_id.clone(),
                            release: entry.release
                        }),
                        ready && !entry.is_default
                    ),
                ]
                .spacing(4),
            );
            if entry.is_default {
                content = content.push(control(
                    "Update from bundle…",
                    Action::PickBundle(Some(entry.plugin_id.clone())),
                    ready,
                ));
            }
            content = content.push(control(
                "Remove registration…",
                Action::Remove {
                    plugin_id: entry.plugin_id.clone(),
                    release: entry.release,
                },
                ready,
            ));
        }
    }
    if let Some(inspection) = &manager.inspection {
        content = content.push(text(inspection).size(12));
    }
    if let Some(removal) = &manager.removal {
        let confirmed = ready && model.plugin_references.settled(&model.document);
        content = content.push(text(format!("Remove {}\n{}?", removal.plugin_id, removal.release)).size(14))
            .push(text("This de-registers one release, not its cached bytes. Exact experiment pins remain unchanged and may become unavailable. Other open sessions may retain it; acknowledgement permits de-registration, never deletion under readers. Choose another default first if other releases remain.").size(12));
        if let Some(usage) = model
            .plugin_references
            .report
            .as_ref()
            .and_then(|r| r.releases.get(&removal.release))
        {
            content = content.push(
                text(format!(
                    "This session retains it: current={}, undo/redo={}, accepted requests={}.",
                    usage.current, usage.history, usage.receipts
                ))
                .size(12),
            );
        }
        content = content
            .push(control(
                "Remove only if unused",
                Action::ConfirmRemoval(false),
                confirmed,
            ))
            .push(control(
                "Acknowledge open references and remove",
                Action::ConfirmRemoval(true),
                confirmed,
            ))
            .push(control("Cancel removal", Action::CancelRemoval, true));
    }
    content = content.push(text("Pack/validate remain CLI operations. No kernels are executed by this panel. Reference leases cover known inventory-aware sessions, not every file on disk.").size(11));
    container(scrollable(content))
        .width(Length::Fixed(400.0))
        .height(Length::Fill)
        .padding(10)
        .into()
}
