//! Numeric observation controls, independent of editable experiment state.
use crate::{
    message::Message,
    model::Model,
    run::{Action as Run, fields::Action},
};
use iced::{
    Element,
    widget::{button, checkbox, column, text, text_input},
};

pub(super) fn view(model: &Model) -> Element<'_, Message> {
    let inspector = &model.run.fields;
    let ready = model.run.attached() && !model.run.is_pending() && model.run.command().is_none();
    let message = |action| Message::Run(Run::Field(action));
    let mut content = column![
        text("Field inspection / point sampling").size(16),
        text("Read-only, at the displayed run boundary. Enter the configured field instance ID, not a plugin/family name. No installed plugin is needed.").size(11),
        text_input("Exact field instance", &inspector.instance).on_input(move |v| message(Action::Instance(v))),
        button("Inspect this field").on_press_maybe(ready.then_some(message(Action::Inspect))),
    ].spacing(6);
    if let Some(field) = inspector.descriptor() {
        content = content.push(text(format!("Source: {:?}\nKernel: {}\nPrecision: {:?}\nState: {}\nChannels below retain their declared shape, SI dimension and frame.", field.snapshot.source, field.context.kernel, field.context.compute_precision, field.snapshot.state.digest)).size(10));
        for (index, binding) in field.context.observables.iter().enumerate() {
            let generation = inspector.generation();
            content = content.push(
                checkbox(inspector.channel_selected(index))
                    .label(format!(
                        "{} — {:?}, {:?}; frame {}",
                        binding.slot,
                        binding.channel.schema.shape,
                        binding.channel.schema.dimension,
                        binding.channel.schema.frame
                    ))
                    .text_size(11)
                    .on_toggle(move |_| message(Action::Channel { generation, index })),
            );
            if inspector.channel_selected(index) {
                content = content.push(
                    text(format!(
                        "{}\nAxes: {:?}\nConventions: {}",
                        binding.channel.schema.meaning,
                        binding.channel.schema.axes,
                        binding.channel.schema.conventions
                    ))
                    .size(10),
                );
            }
        }
        content = content.push(text("Points in metres: x,y,z; x,y,z (up to 4096). Point IDs follow input order. Sampling does not pin or refresh the boundary.").size(11))
            .push(text_input("0,0,0; 1,0,0", &inspector.points).on_input(move |v| message(Action::Points(v))))
            .push(button("Sample selected channels").on_press_maybe((ready && inspector.has_channels()).then_some(message(Action::Sample))));
    }
    if let Some(report) = inspector.report() {
        content = content.push(text(format!("Validated complete query: {} points, {} cells. Showing {} cells (at most 16 values per cell).\nRequest {}\nResponse {}\nQuality flags: 1 direct, 2 interpolated, 4 reconstructed. Invalid cells have no numeric value.", report.points, report.cells, report.rows.len(), report.request_digest, report.response_digest)).size(10));
        content = content.push(text("Direction glyphs are normalized, not magnitude-scaled; green does not encode quality. Drawing explicitly maps the selected channel's components to world X/Y/Z: verify its declared frame, axes and conventions first. Unknown/non-Cartesian frames require a transform and must not be mapped directly.").size(11))
            .push(text_input("Arrow length in metres (presentation only)", &inspector.vector_length).on_input(move |v| message(Action::VectorLength(v))));
        for (channel, binding) in report.metadata.channels.iter().enumerate() {
            if binding.schema.shape == (orishu_plugin::Shape::Vector { length: 3 }) {
                let generation = inspector.generation();
                content = content
                    .push(
                        text(format!(
                            "{} — frame: {}; axes: {:?}; conventions: {}",
                            binding.schema.name,
                            binding.schema.frame,
                            binding.schema.axes,
                            binding.schema.conventions
                        ))
                        .size(10),
                    )
                    .push(
                        button("Map this channel to world X/Y/Z and draw directions")
                            .on_press_maybe(ready.then_some(message(Action::Vectors {
                                generation,
                                channel,
                            }))),
                    );
            }
        }
        if let Some(vectors) = inspector.vectors() {
            content = content.push(text(format!("{}: {} direction arrows / {} points; length {} m, NOT field magnitude. {} invalid, {} zero vectors, {} outside scale/precision. Uses all queried points, not the numeric table's truncation. Camera-clipped and subpixel directions may not be visible.", report.metadata.channels[vectors.channel].schema.name, vectors.arrows.arrows().len(), vectors.points, vectors.length_metres, vectors.invalid, vectors.zero, vectors.omitted_scale)).size(11));
        }
        content = content.push(button("Hide directions").on_press(message(Action::HideVectors)));
        for row in &report.rows {
            content = content.push(text(&row.display).size(11));
        }
    }
    content.into()
}
