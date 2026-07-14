use crate::{
    manifest::{parse_resource_documents, resolve_deployment_plan},
    DeploymentPlan, LoaderError, LoaderErrorKind,
};
use std::env;

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

        let manifest = expand_environment_variables(manifest)?;
        resolve_deployment_plan(parse_resource_documents(&manifest)?)
    }
}

fn expand_environment_variables(manifest: &str) -> Result<String, LoaderError> {
    expand_environment_variables_with(manifest, |name| {
        env::var(name).map_err(|error| match error {
            env::VarError::NotPresent => format!("environment variable {name} is not set"),
            env::VarError::NotUnicode(_) => {
                format!("environment variable {name} is not valid UTF-8")
            }
        })
    })
}

fn expand_environment_variables_with<F>(
    manifest: &str,
    mut resolve: F,
) -> Result<String, LoaderError>
where
    F: FnMut(&str) -> Result<String, String>,
{
    let mut expanded = String::with_capacity(manifest.len());
    let mut cursor = 0;

    while let Some(relative_start) = manifest[cursor..].find("{{") {
        let start = cursor + relative_start;
        expanded.push_str(&manifest[cursor..start]);
        let name_start = start + 2;
        let Some(relative_end) = manifest[name_start..].find("}}") else {
            return Err(LoaderError::new(
                LoaderErrorKind::EnvironmentVariable,
                "unterminated environment placeholder; expected `}}`",
            ));
        };
        let end = name_start + relative_end;
        let name = &manifest[name_start..end];
        if name.is_empty() {
            return Err(LoaderError::new(
                LoaderErrorKind::EnvironmentVariable,
                "environment placeholder must contain a variable name",
            ));
        }
        let value = resolve(name).map_err(|error| {
            LoaderError::new(LoaderErrorKind::EnvironmentVariable, error)
        })?;
        expanded.push_str(&value);
        cursor = end + 2;
    }

    expanded.push_str(&manifest[cursor..]);
    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_environment_placeholders() {
        let expanded = expand_environment_variables_with(
            "password: \"{{PASSWORD}}\"",
            |name| Ok(format!("value-for-{name}")),
        )
        .expect("expanded manifest");
        assert_eq!(expanded, "password: \"value-for-PASSWORD\"");
    }

    #[test]
    fn rejects_missing_environment_variables() {
        let error = expand_environment_variables_with("{{PASSWORD}}", |_| {
            Err("environment variable PASSWORD is not set".to_string())
        })
        .expect_err("missing environment variable");
        assert_eq!(error.kind, LoaderErrorKind::EnvironmentVariable);
        assert!(error.message.contains("PASSWORD is not set"));
    }

    #[test]
    fn rejects_empty_environment_placeholders() {
        let error = expand_environment_variables_with("{{}}", |_| {
            Ok("value".to_string())
        })
        .expect_err("empty environment placeholder");
        assert_eq!(error.kind, LoaderErrorKind::EnvironmentVariable);
    }
}
