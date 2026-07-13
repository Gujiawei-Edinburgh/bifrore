use tenon_loader::{Loader, LoaderErrorKind};
use tenon_message::{daemon::v1::json_path_segment, plan::auth_plan, plan::AccessMode};

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

    let first_auth = plan.sources[0]
        .auth
        .as_ref()
        .and_then(|auth| auth.kind.as_ref())
        .expect("first auth");
    assert!(matches!(
        first_auth,
        auth_plan::Kind::Script(script) if script.source.contains("function credentials")
    ));

    let script_auth = plan.sources[1]
        .auth
        .as_ref()
        .and_then(|auth| auth.kind.as_ref())
        .expect("script auth");
    assert!(matches!(script_auth, auth_plan::Kind::Script(script) if script.source.contains("function credentials")));

    let process = plan.process.as_ref().expect("process plan");
    assert!(process.source.contains("function on_message"));
    let access = process.access_plan.as_ref().expect("access plan");

    let source = access.source.as_ref().expect("source access");
    assert_eq!(source.mode, AccessMode::Selective as i32);
    assert!(source.name);
    assert!(!source.version);

    let topic = access.topic.as_ref().expect("topic access");
    assert_eq!(topic.mode, AccessMode::Selective as i32);
    assert!(topic.raw);
    assert!(!topic.levels);
    assert_eq!(topic.level_indexes, vec![2]);

    let payload = access.payload.as_ref().expect("payload access");
    assert_eq!(payload.mode, AccessMode::Selective as i32);
    let payload_paths = payload
        .paths
        .iter()
        .map(|path| {
            path.segments
                .iter()
                .map(|segment| match segment.kind.as_ref().expect("path segment") {
                    json_path_segment::Kind::Field(field) => field.as_str(),
                    json_path_segment::Kind::Index(_) => "<index>",
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(payload_paths, vec![vec!["hum"], vec!["temp"]]);

    let metadata = access.metadata.as_ref().expect("metadata access");
    assert_eq!(metadata.mode, AccessMode::Selective as i32);
    assert!(metadata.qos);
    assert!(!metadata.pkid);
    assert!(!metadata.retain);
    assert!(!metadata.dup);

    let properties = access.properties.as_ref().expect("properties access");
    assert_eq!(properties.mode, AccessMode::None as i32);

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
