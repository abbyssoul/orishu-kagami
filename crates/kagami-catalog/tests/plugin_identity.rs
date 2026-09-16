use kagami_catalog::{ComponentTypeId, Limits, Template, document};
use orishu_plugin::ContributionRef;

fn reference(digit: char) -> ContributionRef {
    ContributionRef {
        release: format!("sha256:{}", digit.to_string().repeat(64))
            .parse()
            .unwrap(),
        extension_point: "orishu.model.components/v1".parse().unwrap(),
        local_id: "gravity-mass".parse().unwrap(),
    }
}
#[test]
fn exact_identity_is_provider_qualified_and_cannot_be_confused_with_legacy_names() {
    let a = ComponentTypeId::exact(reference('a')).unwrap();
    let b = ComponentTypeId::exact(reference('b')).unwrap();
    assert_ne!(a, b);
    assert_eq!(a.local_name().as_str(), "gravity_mass");
    let text = serde_yaml::to_string(&a).unwrap();
    assert_eq!(serde_yaml::from_str::<ComponentTypeId>(&text).unwrap(), a);
    let legacy: ComponentTypeId =
        serde_yaml::from_str("plugin: org.example.gravity\nname: gravity_mass").unwrap();
    assert_ne!(a, legacy);
    assert!(legacy.contribution().is_none());
    let mut wrong = reference('a');
    wrong.extension_point = "orishu.model.fields/v1".parse().unwrap();
    assert!(ComponentTypeId::exact(wrong).is_err());
    assert!(
        serde_yaml::from_str::<ComponentTypeId>(&text.replace("components/v1", "fields/v1"))
            .is_err()
    );
    assert!(
        serde_yaml::from_str::<ComponentTypeId>(&format!(
            "{text}\nplugin: org.example.gravity\nname: mass"
        ))
        .is_err()
    );
}
#[test]
fn new_template_version_keeps_aliases_separate_and_refuses_duplicate_types() {
    let spec = document::SpecDocument {
        components: vec![
            document::ComponentDocument {
                component_type: ComponentTypeId::exact(reference('a')).unwrap(),
                name: Some("first_mass".try_into().unwrap()),
                properties: Default::default(),
            },
            document::ComponentDocument {
                component_type: ComponentTypeId::exact(reference('b')).unwrap(),
                name: Some("second_mass".try_into().unwrap()),
                properties: Default::default(),
            },
        ],
        ..Default::default()
    };
    let doc = document::new(
        document::MetadataDocument::new("test".try_into().unwrap(), "object".try_into().unwrap()),
        spec,
    );
    assert_eq!(doc.api_version().as_str(), document::PINNED_API_VERSION);
    let template = Template::from_document(&doc, &Limits::default()).unwrap();
    assert_eq!(
        template.spec.components[0].local_name().as_str(),
        "first_mass"
    );
    let text = serde_yaml::to_string(&template.to_document()).unwrap();
    let decoded = serde_yaml::from_str(&text).unwrap();
    assert_eq!(
        Template::from_document(&decoded, &Limits::default()).unwrap(),
        template
    );
    let old =
        serde_yaml::from_str(&text.replace("kagami.catalog/v2", "kagami.catalog/v1")).unwrap();
    assert!(Template::from_document(&old, &Limits::default()).is_err());
    let mut duplicate = doc.clone();
    duplicate.spec.components[1].component_type =
        duplicate.spec.components[0].component_type.clone();
    assert!(Template::from_document(&duplicate, &Limits::default()).is_err());
}
