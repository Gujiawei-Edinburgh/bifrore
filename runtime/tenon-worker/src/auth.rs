use crate::{WorkerError, WorkerResult};
use mlua::{Function, Lua, LuaOptions, StdLib, Table, Value};
use std::env;
use tenon_extension::{AuthResult, SourceContext, AUTH_CREDENTIALS_FN};
use tenon_message::plan::{auth_plan, AuthPlan};

pub(crate) fn resolve_auth(
    auth: Option<&AuthPlan>,
    source: &SourceContext,
) -> WorkerResult<Option<AuthResult>> {
    let Some(auth) = auth else {
        return Ok(None);
    };

    let kind = auth
        .kind
        .as_ref()
        .ok_or_else(|| WorkerError::mqtt("MQTT auth plan kind is missing"))?;
    match kind {
        auth_plan::Kind::None(_) => Ok(None),
        auth_plan::Kind::Script(script) => evaluate_script(&script.source, source)
            .and_then(expand_auth_result)
            .map(Some),
    }
}

fn expand_auth_result(result: AuthResult) -> WorkerResult<AuthResult> {
    match result {
        AuthResult::UsernamePassword { username, password } => Ok(
            AuthResult::username_password(expand_auth_value(&username)?, expand_auth_value(&password)?),
        ),
        AuthResult::ClientCertificate {
            cert_path,
            key_path,
            ca_path,
        } => Ok(AuthResult::client_certificate(
            expand_auth_value(&cert_path)?,
            expand_auth_value(&key_path)?,
            ca_path
                .as_deref()
                .map(expand_auth_value)
                .transpose()?,
        )),
        AuthResult::Custom {
            username,
            password,
            properties,
        } => Ok(AuthResult::custom(
            username.as_deref().map(expand_auth_value).transpose()?,
            password.as_deref().map(expand_auth_value).transpose()?,
            properties
                .into_iter()
                .map(|(key, value)| Ok((key, expand_auth_value(&value)?)))
                .collect::<WorkerResult<Vec<_>>>()?,
        )),
    }
}

fn expand_auth_value(value: &str) -> WorkerResult<String> {
    let mut expanded = String::with_capacity(value.len());
    let mut cursor = 0;
    while let Some(relative_start) = value[cursor..].find("{{") {
        let start = cursor + relative_start;
        expanded.push_str(&value[cursor..start]);
        let name_start = start + 2;
        let Some(relative_end) = value[name_start..].find("}}") else {
            return Err(WorkerError::mqtt(
                "unterminated authentication environment placeholder; expected `}}`",
            ));
        };
        let end = name_start + relative_end;
        let name = &value[name_start..end];
        if name.is_empty() {
            return Err(WorkerError::mqtt(
                "authentication environment placeholder must contain a variable name",
            ));
        }
        let replacement = env::var(name).map_err(|error| match error {
            env::VarError::NotPresent => {
                WorkerError::mqtt(format!("environment variable {name} is not set"))
            }
            env::VarError::NotUnicode(_) => {
                WorkerError::mqtt(format!("environment variable {name} is not valid UTF-8"))
            }
        })?;
        expanded.push_str(&replacement);
        cursor = end + 2;
    }
    expanded.push_str(&value[cursor..]);
    Ok(expanded)
}

fn evaluate_script(source: &str, context: &SourceContext) -> WorkerResult<AuthResult> {
    let lua = Lua::new_with(safe_lua_libs(), LuaOptions::default())
        .map_err(|error| WorkerError::mqtt(format!("failed to create auth Lua runtime: {error}")))?;
    lua.load(source).exec().map_err(|error| {
        WorkerError::mqtt(format!("failed to load MQTT auth script: {error}"))
    })?;
    let credentials: Function = lua
        .globals()
        .get(AUTH_CREDENTIALS_FN)
        .map_err(|error| WorkerError::mqtt(format!("invalid MQTT auth script: {error}")))?;
    let ctx = create_context(&lua, context)?;
    let result: Value = credentials
        .call(ctx)
        .map_err(|error| WorkerError::mqtt(format!("MQTT auth script failed: {error}")))?;
    parse_auth_result(result)
}

fn create_context(lua: &Lua, source: &SourceContext) -> WorkerResult<Table> {
    let ctx = lua
        .create_table()
        .map_err(|error| WorkerError::mqtt(format!("failed to create auth context: {error}")))?;
    let source_table = lua
        .create_table()
        .map_err(|error| WorkerError::mqtt(format!("failed to create auth source: {error}")))?;
    source_table
        .set("name", source.name.clone())
        .and_then(|_| source_table.set("version", source.version.clone()))
        .map_err(|error| WorkerError::mqtt(format!("failed to build auth source: {error}")))?;
    ctx.set("source", source_table)
        .map_err(|error| WorkerError::mqtt(format!("failed to build auth context: {error}")))?;
    Ok(ctx)
}

fn parse_auth_result(value: Value) -> WorkerResult<AuthResult> {
    let table = match value {
        Value::Table(table) => table,
        other => {
            return Err(WorkerError::mqtt(format!(
                "MQTT auth script must return a table, got {}",
                other.type_name()
            )))
        }
    };
    let auth_type: String = table_field(&table, "type")?;
    match auth_type.as_str() {
        "username-password" => Ok(AuthResult::username_password(
            table_field::<String>(&table, "username")?,
            table_field::<String>(&table, "password")?,
        )),
        "client-certificate" => Ok(AuthResult::client_certificate(
            table_field::<String>(&table, "cert_path")?,
            table_field::<String>(&table, "key_path")?,
            optional_table_field::<String>(&table, "ca_path")?,
        )),
        "custom" => Ok(AuthResult::custom(
            optional_table_field::<String>(&table, "username")?,
            optional_table_field::<String>(&table, "password")?,
            parse_properties(&table)?,
        )),
        other => Err(WorkerError::mqtt(format!(
            "MQTT auth script returned unknown credential type: {other}"
        ))),
    }
}

fn table_field<T>(table: &Table, name: &str) -> WorkerResult<T>
where
    T: mlua::FromLua,
{
    table
        .get(name)
        .map_err(|error| WorkerError::mqtt(format!("invalid MQTT auth field {name}: {error}")))
}

fn optional_table_field<T>(table: &Table, name: &str) -> WorkerResult<Option<T>>
where
    T: mlua::FromLua,
{
    table
        .get(name)
        .map_err(|error| WorkerError::mqtt(format!("invalid MQTT auth field {name}: {error}")))
}

fn parse_properties(table: &Table) -> WorkerResult<Vec<(String, String)>> {
    let value: Value = table
        .get("properties")
        .map_err(|error| WorkerError::mqtt(format!("invalid MQTT auth properties: {error}")))?;
    let Some(properties) = (match value {
        Value::Nil => None,
        Value::Table(properties) => Some(properties),
        other => {
            return Err(WorkerError::mqtt(format!(
                "MQTT auth properties must be a table, got {}",
                other.type_name()
            )))
        }
    }) else {
        return Ok(Vec::new());
    };

    let mut properties = properties
        .pairs::<String, String>()
        .map(|entry| {
            entry.map_err(|error| WorkerError::mqtt(format!("invalid MQTT auth property: {error}")))
        })
        .collect::<WorkerResult<Vec<_>>>()?;
    properties.sort_unstable();
    Ok(properties)
}

fn safe_lua_libs() -> StdLib {
    StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceContext {
        SourceContext::new("source", "v1")
    }

    #[test]
    fn evaluates_username_password_script() {
        let result = evaluate_script(
            r#"
function credentials(ctx)
  return {
    type = "username-password",
    username = "dev",
    password = "dev"
  }
end
"#,
            &source(),
        )
        .expect("auth result");

        assert_eq!(result, AuthResult::username_password("dev", "dev"));
    }

    #[test]
    fn evaluates_custom_script_properties() {
        let result = evaluate_script(
            r#"
function credentials(ctx)
  return {
    type = "custom",
    username = "device",
    password = "signature",
    properties = { timestamp = "123", signature = "signature" }
  }
end
"#,
            &source(),
        )
        .expect("auth result");

        assert_eq!(
            result,
            AuthResult::custom(
                Some("device".to_string()),
                Some("signature".to_string()),
                vec![
                    ("signature".to_string(), "signature".to_string()),
                    ("timestamp".to_string(), "123".to_string()),
                ],
            )
        );
    }

    #[test]
    fn expands_environment_values_after_auth_result_parsing() {
        std::env::set_var("TENON_TEST_USERNAME", "dev");
        std::env::set_var("TENON_TEST_PASSWORD", "dev");
        std::env::set_var("TENON_TEST_PROPERTY", "signature");

        let result = evaluate_script(
            r#"
function credentials(ctx)
  return {
    type = "custom",
    username = "{{TENON_TEST_USERNAME}}",
    password = "{{TENON_TEST_PASSWORD}}",
    properties = {
      mechanism = "{{TENON_TEST_PROPERTY}}"
    }
  }
end
"#,
            &source(),
        )
        .and_then(expand_auth_result)
            .expect("auth result");

        assert_eq!(
            result,
            AuthResult::custom(
                Some("dev".to_string()),
                Some("dev".to_string()),
                vec![("mechanism".to_string(), "signature".to_string())],
            )
        );

        std::env::remove_var("TENON_TEST_USERNAME");
        std::env::remove_var("TENON_TEST_PASSWORD");
        std::env::remove_var("TENON_TEST_PROPERTY");
    }

    #[test]
    fn rejects_non_table_result() {
        let error = evaluate_script(
            "function credentials(ctx) return true end",
            &source(),
        )
        .expect_err("invalid auth result");
        assert!(error.message.contains("must return a table"));
    }
}
