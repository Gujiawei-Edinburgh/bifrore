use crate::{
    manifest::{parse_resource_documents, ResourceDocument},
    LoaderError, LoaderErrorKind,
};

#[derive(Debug, Clone, Default)]
pub struct Loader;

pub fn load(manifest: &str) -> Result<Vec<ResourceDocument>, LoaderError> {
    Loader.load(manifest)
}

impl Loader {
    pub fn load(&self, manifest: &str) -> Result<Vec<ResourceDocument>, LoaderError> {
        if manifest.trim().is_empty() {
            return Err(LoaderError::new(
                LoaderErrorKind::EmptyManifest,
                "pipeline manifest is empty",
            ));
        }

        parse_resource_documents(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResourceKind;

    #[test]
    fn rejects_empty_manifest() {
        let err = Loader.load(" \n\t").expect_err("empty manifest");
        assert_eq!(err.kind, LoaderErrorKind::EmptyManifest);
    }

    #[test]
    fn loads_pipeline_manifest() {
        let resources = Loader
            .load(include_str!("../../../examples/pipeline/sensor-pipeline.yaml"))
            .expect("pipeline manifest");

        assert_eq!(resources.len(), 6);
        assert!(resources
            .iter()
            .any(|resource| resource.kind == ResourceKind::Pipeline
                && resource.metadata.name == "sensor-ingest"
                && resource.metadata.version == "v1"));
    }

    #[test]
    fn accepts_unresolved_refs() {
        let resources = Loader
            .load(
                r#"
apiVersion: tenon.apache.org/v1alpha1
kind: Pipeline
metadata:
  name: missing-source
  version: v1
spec:
  execution:
    mode: intra-proc
  sourceRefs:
    - kind: MqttSource
      name: missing
      version: v1
  processRef:
    kind: Process
    name: missing
    version: v1
  egressRef:
    kind: Egress
    name: missing
    version: v1
"#,
            )
            .expect("loader should not resolve refs");

        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].kind, ResourceKind::Pipeline);
    }
}
