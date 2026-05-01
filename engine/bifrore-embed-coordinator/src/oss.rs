use crate::{ClientIdSink, ClientIdSource, EngineCoordinator, RuleSource};
use std::fs;
use std::path::Path;

pub struct OssCoordinator {
    rule_path: String,
    client_ids_path: String,
}

impl OssCoordinator {
    pub fn from_files(rule_path: String, client_ids_path: String) -> Self {
        Self {
            rule_path,
            client_ids_path,
        }
    }

    pub fn into_engine_coordinator(self) -> EngineCoordinator {
        EngineCoordinator::new(
            Box::new(FileRuleSource::new(self.rule_path)),
            Box::new(FileClientIdStore::new(self.client_ids_path.clone())),
            Box::new(FileClientIdStore::new(self.client_ids_path)),
        )
    }
}

#[derive(Debug)]
struct FileRuleSource {
    path: String,
}

impl FileRuleSource {
    fn new(path: String) -> Self {
        Self { path }
    }
}

impl RuleSource for FileRuleSource {
    fn load_rules(&self) -> Result<Vec<u8>, String> {
        fs::read(&self.path).map_err(|err| err.to_string())
    }

    fn label(&self) -> &str {
        &self.path
    }
}

#[derive(Debug)]
struct FileClientIdStore {
    path: String,
}

impl FileClientIdStore {
    fn new(path: String) -> Self {
        Self { path }
    }
}

impl ClientIdSource for FileClientIdStore {
    fn load_client_ids(&self) -> Result<Option<Vec<String>>, String> {
        let content = match fs::read(&self.path) {
            Ok(content) => content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.to_string()),
        };
        Ok(parse_client_ids(&content))
    }
}

impl ClientIdSink for FileClientIdStore {
    fn persist_client_ids(&self, client_ids: &[String]) -> Result<(), String> {
        if client_ids.is_empty() {
            return Ok(());
        }
        let path_ref = Path::new(&self.path);
        if let Some(parent) = path_ref.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
        }
        fs::write(path_ref, format!("{}\n", client_ids.join("\n")))
            .map_err(|err| err.to_string())
    }

    fn label(&self) -> &str {
        &self.path
    }
}

fn parse_client_ids(bytes: &[u8]) -> Option<Vec<String>> {
    let content = std::str::from_utf8(bytes).ok()?;
    let values = content
        .lines()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_client_ids_from_file_when_present() {
        let path = "/tmp/bifrore-test-client-ids";
        let _ = fs::write(path, "cid-a\ncid-b\n");
        let coordinator = OssCoordinator::from_files(
            "/tmp/bifrore-test-rules-missing".to_string(),
            path.to_string(),
        )
        .into_engine_coordinator();
        let ids = coordinator.resolve_client_ids("node-1", 2);
        assert_eq!(ids, vec!["cid-a".to_string(), "cid-b".to_string()]);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn generates_client_ids_when_file_is_missing() {
        let coordinator = OssCoordinator::from_files(
            "/tmp/bifrore-test-rules-missing".to_string(),
            "/tmp/bifrore-test-client-ids-missing".to_string(),
        )
        .into_engine_coordinator();
        let ids = coordinator.resolve_client_ids("node-1", 3);
        assert_eq!(
            ids,
            vec![
                "node-1_0".to_string(),
                "node-1_1".to_string(),
                "node-1_2".to_string()
            ]
        );
    }
}
