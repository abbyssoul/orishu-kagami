//! The inspector, driven entirely by registered schemas.
//!
//! This file contains no knowledge of any specific component and branches on
//! no component name. It walks the object's components, asks the registry what
//! each one declares, and renders a field per property; then it offers every
//! registered-but-unattached component type for attachment. That is Field CAD
//! ADR 0021's exit criterion, and it is what makes a new plugin's component
//! authorable the moment its schema is registered — with no change here.
//!
//! # Expressions are edited as text
//!
//! A quantity field shows the *authored source*, not the number it resolved
//! to, and the resolved value is displayed beside it as derived output
//! (ADR 0005). Typing is client-local until it is committed, so an
//! expression becomes a revision when it is finished rather than per
//! keystroke.

use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};
use kagami_catalog::{ComponentTypeId, PropertyName, PropertySchema};
use kagami_document::{CapabilityGap, Object, ObjectComponent, ObjectId, PropertyValue};

use super::section::view_section;
use crate::message::{Authoritative, ClientLocal, Message};
use crate::model::Model;

pub fn view(model: &Model) -> Element<'_, Message> {
    let snapshot = model.document.snapshot();
    let content: Element<Message> = match model
        .selected
        .and_then(|id| snapshot.object(id).map(|object| (id, object)))
    {
        Some((id, object)) => scrollable(view_object(model, id, object))
            .height(Length::Fill)
            .into(),
        None => text("Nothing selected").into(),
    };

    container(column![content, notice(model)].spacing(8))
        .width(Length::Fixed(300.0))
        .height(Length::Fill)
        .padding(8)
        .into()
}

fn view_object<'a>(model: &'a Model, id: ObjectId, object: &'a Object) -> Element<'a, Message> {
    let mut sections = column![view_section("Object", view_identity(id, object))].spacing(12);

    for (type_id, component) in &object.components {
        sections = sections.push(view_section(
            &component_title(type_id),
            view_component(model, id, type_id, component),
        ));
    }

    if let Some(attachable) = view_attachable(model, id, object) {
        sections = sections.push(view_section("Add component", attachable));
    }

    if let Some(provenance) = &object.provenance {
        // Historical evidence, not a link (ADR 0008). Shown so a user can see
        // where an object came from, with nothing offering to refresh it.
        sections = sections.push(view_section(
            "From template",
            text(format!("{} (copied)", provenance.identity))
                .size(12)
                .into(),
        ));
    }

    sections.into()
}

fn view_identity<'a>(id: ObjectId, object: &'a Object) -> Element<'a, Message> {
    column![
        text_input("Name", object.name.as_str())
            .on_input(move |name| Authoritative::RenameObject(id, name).into()),
        text(format!("{id}")).size(11),
    ]
    .spacing(4)
    .into()
}

/// One component's properties, from whatever its schema declares.
fn view_component<'a>(
    model: &'a Model,
    object: ObjectId,
    type_id: &'a ComponentTypeId,
    component: &'a ObjectComponent,
) -> Element<'a, Message> {
    let mut rows = column![].spacing(6);

    // A component the installed schemas cannot govern keeps its authored
    // values and says why, rather than being hidden or given a substitute.
    if let Some(gap) = model.document.capabilities().gap(object, type_id) {
        rows = rows.push(text(gap_notice(gap)).size(11));
    }

    match model.document.schemas().get(type_id) {
        Some(schema) => {
            for (name, declared) in &schema.properties {
                rows = rows.push(view_property(
                    model,
                    object,
                    type_id,
                    name,
                    declared,
                    component.properties.get(name),
                ));
            }
        }
        None => {
            // No declaration here, so the authored values are shown as they
            // were written and nothing is offered to edit them with.
            for (name, value) in &component.properties {
                rows =
                    rows.push(text(format!("{name}: {}", value.source().unwrap_or("—"))).size(12));
            }
        }
    }

    rows = rows.push(
        button(text("Remove component").size(11))
            .on_press(Authoritative::DetachComponent(object, type_id.clone()).into())
            .padding(4),
    );
    rows.into()
}

/// One property field: the authored source, and what it resolved to.
fn view_property<'a>(
    model: &'a Model,
    object: ObjectId,
    component: &'a ComponentTypeId,
    property: &'a PropertyName,
    declared: &'a PropertySchema,
    stored: Option<&'a PropertyValue>,
) -> Element<'a, Message> {
    let editing = model.editing.as_ref().filter(|edit| {
        edit.object == object && &edit.component == component && &edit.property == property
    });
    let source = match editing {
        Some(edit) => edit.source.clone(),
        None => stored
            .and_then(|value| value.source())
            .unwrap_or_default()
            .to_owned(),
    };

    let (object_id, component_id, property_id) = (object, component.clone(), property.clone());
    let (commit_component, commit_property) = (component.clone(), property.clone());
    let field = text_input(declared.kind.label(), &source)
        .on_input(move |source| {
            ClientLocal::EditProperty {
                object: object_id,
                component: component_id.clone(),
                property: property_id.clone(),
                source,
            }
            .into()
        })
        .on_submit(
            Authoritative::SetProperty {
                object,
                component: commit_component,
                property: commit_property,
                source: source.clone(),
            }
            .into(),
        );

    // The resolved value is *derived output*, shown beside the intent rather
    // than in place of it.
    let resolved: Element<'_, Message> = match stored {
        Some(PropertyValue::Quantity {
            si_value,
            dimension,
            ..
        }) => text(format!("= {si_value} {dimension}")).size(11).into(),
        Some(value) if !value.is_priced() => text("= not priced here").size(11).into(),
        Some(PropertyValue::Boolean(value)) => text(format!("= {value}")).size(11).into(),
        Some(PropertyValue::Text(value)) => text(format!("= {value}")).size(11).into(),
        _ => text("required").size(11).into(),
    };

    column![
        text(format!(
            "{property}{}",
            if declared.required { " *" } else { "" }
        ))
        .size(12),
        field,
        resolved,
    ]
    .spacing(2)
    .into()
}

/// Every registered component type the object does not already carry.
fn view_attachable<'a>(
    model: &'a Model,
    object: ObjectId,
    carried: &'a Object,
) -> Option<Element<'a, Message>> {
    let mut buttons = column![].spacing(4);
    let mut any = false;
    for schema in model.document.schemas().schemas() {
        if carried.components.contains_key(&schema.type_id) {
            continue;
        }
        any = true;
        buttons = buttons.push(
            button(text(component_title(&schema.type_id)).size(11))
                .on_press(Authoritative::AttachComponent(object, schema.type_id.clone()).into())
                .width(Length::Fill)
                .padding(4),
        );
    }
    any.then(|| buttons.into())
}

/// A component type's name, as a heading.
///
/// The plugin-qualified identity, rendered. Not a lookup table of friendly
/// names — one of those would be exactly the app-side knowledge this file
/// exists without.
fn component_title(type_id: &ComponentTypeId) -> String {
    type_id.to_string()
}

fn gap_notice(gap: &CapabilityGap) -> String {
    match gap.is_absent() {
        true => "Plugin not installed — values preserved.".to_owned(),
        false => format!("Not usable here: {gap}"),
    }
}

/// The most recent refusal, if anything was refused.
fn notice(model: &Model) -> Element<'_, Message> {
    match &model.document.notice {
        Some(message) => row![
            text(message.clone()).size(11).width(Length::Fill),
            button(text("×").size(11))
                .on_press(ClientLocal::DismissNotice.into())
                .padding(2),
        ]
        .spacing(4)
        .into(),
        None => Space::new().into(),
    }
}
