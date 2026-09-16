//! Declarative reference vocabulary, shared by the independently built guest and
//! packaging/test tooling. No field computation or native plugin registration.
use orishu_plugin::{
    Declaration, Dimension, Limits, ObservableSchema, Payload, Shape,
    execution::{ObservableBinding, SampleChannel},
};
pub fn bindings() -> Vec<ObservableBinding> {
    [
        (
            "acceleration",
            Shape::Vector { length: 3 },
            Dimension::new([1, 0, -2, 0, 0, 0, 0]),
            "Newtonian gravitational acceleration",
            vec!["world x/y/z".into()],
            "Cartesian vector in the world frame",
        ),
        (
            "jacobian",
            Shape::Matrix {
                rows: 3,
                columns: 3,
            },
            Dimension::new([0, 0, -2, 0, 0, 0, 0]),
            "Spatial Jacobian of gravitational acceleration",
            vec![
                "acceleration component x/y/z".into(),
                "world coordinate x/y/z".into(),
            ],
            "Row-major J[i,j] = d(acceleration_i)/d(world_coordinate_j)",
        ),
        (
            "potential",
            Shape::Scalar,
            Dimension::new([2, 0, -2, 0, 0, 0, 0]),
            "Newtonian gravitational scalar potential",
            vec![],
            "Zero potential at infinity for isolated point sources",
        ),
    ]
    .into_iter()
    .map(|(slot, shape, dimension, meaning, axes, conventions)| {
        let schema = ObservableSchema {
            name: format!("org.orishu.reference.gravity.{slot}")
                .parse()
                .expect("static name"),
            version: 1.try_into().expect("nonzero"),
            requirements: vec![],
            meaning: meaning.into(),
            shape,
            dimension,
            frame: "world Cartesian SI".into(),
            axes,
            conventions: conventions.into(),
        };
        let contract = Payload::Observables(Declaration {
            scientific: schema.clone(),
            presentation: None,
        })
        .contract_ref(&Limits::default())
        .expect("static reference schema");
        ObservableBinding {
            slot: slot.parse().expect("static slot"),
            channel: SampleChannel { contract, schema },
            quality_flags: 1,
        }
    })
    .collect()
}
