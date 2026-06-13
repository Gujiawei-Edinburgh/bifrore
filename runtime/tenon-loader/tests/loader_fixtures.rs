use tenon_loader::{DeliveryMode, Loader, LoaderErrorKind, ResourceKind};

fn fixture(path: &str) -> &'static str {
    match path {
        "pipelines/sensor-pipeline.yaml" => {
            include_str!("../test/pipelines/sensor-pipeline.yaml")
        }
        "pipelines/invalid/missing-module.yaml" => {
            include_str!("../test/pipelines/invalid/missing-module.yaml")
        }
        "pipelines/invalid/missing-on-message.yaml" => {
            include_str!("../test/pipelines/invalid/missing-on-message.yaml")
        }
        "pipelines/invalid/invalid-lua-api.yaml" => {
            include_str!("../test/pipelines/invalid/invalid-lua-api.yaml")
        }
        "pipelines/invalid/unsupported-delivery.yaml" => {
            include_str!("../test/pipelines/invalid/unsupported-delivery.yaml")
        }
        _ => panic!("unknown fixture: {path}"),
    }
}

#[test]
fn rejects_empty_manifest() {
    let err = Loader.load(" \n\t").expect_err("empty manifest");
    assert_eq!(err.kind, LoaderErrorKind::EmptyManifest);
}

#[test]
fn loads_sensor_pipeline_fixture() {
    let plan = Loader
        .load(fixture("pipelines/sensor-pipeline.yaml"))
        .expect("valid sensor pipeline");

    assert_eq!(plan.id.kind, ResourceKind::Pipeline);
    assert_eq!(plan.id.name, "sensor-ingest");
    assert_eq!(plan.sources.len(), 2);
    assert_eq!(plan.process.id.name, "sensor-process");
    assert_eq!(plan.process.module.id.name, "sensor-processor");
    assert_eq!(plan.egress.delivery, DeliveryMode::Single);
}

#[test]
fn rejects_missing_module_reference() {
    let error = Loader
        .load(fixture("pipelines/invalid/missing-module.yaml"))
        .expect_err("missing module");

    assert_eq!(error.kind, LoaderErrorKind::ReferenceResolution);
    assert!(error.message.contains("missing Module"));
}

#[test]
fn rejects_module_without_on_message() {
    let error = Loader
        .load(fixture("pipelines/invalid/missing-on-message.yaml"))
        .expect_err("missing on_message");

    assert_eq!(error.kind, LoaderErrorKind::ModuleValidation);
    assert!(error.message.contains("on_message"));
}

#[test]
fn rejects_invalid_lua_extension_api_usage() {
    let error = Loader
        .load(fixture("pipelines/invalid/invalid-lua-api.yaml"))
        .expect_err("invalid Lua API usage");

    assert_eq!(error.kind, LoaderErrorKind::ModuleValidation);
    assert!(error.message.contains("invalid msg field: device"));
}

#[test]
fn rejects_unsupported_delivery_mode() {
    let error = Loader
        .load(fixture("pipelines/invalid/unsupported-delivery.yaml"))
        .expect_err("unsupported delivery");

    assert_eq!(error.kind, LoaderErrorKind::ResourceValidation);
    assert!(error.message.contains("shared"));
}
