use tenon_loader::{Loader, LoaderErrorKind};

fn fixture(path: &str) -> &'static str {
    match path {
        "pipelines/sensor-pipeline.yaml" => {
            include_str!("../test/pipelines/sensor-pipeline.yaml")
        }
        "pipelines/invalid/missing-process.yaml" => {
            include_str!("../test/pipelines/invalid/missing-process.yaml")
        }
        "pipelines/invalid/missing-on-message.yaml" => {
            include_str!("../test/pipelines/invalid/missing-on-message.yaml")
        }
        "pipelines/invalid/invalid-lua-api.yaml" => {
            include_str!("../test/pipelines/invalid/invalid-lua-api.yaml")
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

    let id = plan.id.as_ref().expect("plan id");
    assert_eq!(id.name, "sensor-ingest");
    assert_eq!(plan.sources.len(), 2);

    let process = plan.process.as_ref().expect("process plan");
    assert!(process.source.contains("function on_message"));

    assert!(plan.egress.is_some());
}

#[test]
fn rejects_missing_process_reference() {
    let error = Loader
        .load(fixture("pipelines/invalid/missing-process.yaml"))
        .expect_err("missing process");

    assert_eq!(error.kind, LoaderErrorKind::ReferenceResolution);
    assert!(error.message.contains("missing Process"));
}

#[test]
fn rejects_process_script_without_on_message() {
    let error = Loader
        .load(fixture("pipelines/invalid/missing-on-message.yaml"))
        .expect_err("missing on_message");

    assert_eq!(error.kind, LoaderErrorKind::ScriptValidation);
    assert!(error.message.contains("on_message"));
}

#[test]
fn rejects_invalid_lua_extension_api_usage() {
    let error = Loader
        .load(fixture("pipelines/invalid/invalid-lua-api.yaml"))
        .expect_err("invalid Lua API usage");

    assert_eq!(error.kind, LoaderErrorKind::ScriptValidation);
    assert!(error.message.contains("invalid msg field: device"));
}
