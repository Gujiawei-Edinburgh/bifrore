use crate::{
    manifest::{parse_resource_documents, resolve_deployment_plan},
    DeploymentPlan, LoaderError, LoaderErrorKind,
};

#[derive(Debug, Clone, Default)]
pub struct Loader;

impl Loader {
    pub fn load(&self, manifest: &str) -> Result<DeploymentPlan, LoaderError> {
        if manifest.trim().is_empty() {
            return Err(LoaderError::new(
                LoaderErrorKind::EmptyManifest,
                "pipeline manifest is empty",
            ));
        }

        resolve_deployment_plan(parse_resource_documents(manifest)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_manifest() {
        let err = Loader.load(" \n\t").expect_err("empty manifest");
        assert_eq!(err.kind, LoaderErrorKind::EmptyManifest);
    }
}
