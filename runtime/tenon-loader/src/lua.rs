use std::collections::HashMap;
use tenon_extension::{
    Context, MemoryView, Message, MqttMetadata, PROCESS_ON_MESSAGE_FN, ScriptApi, SourceContext,
    Topic,
};
use tree_sitter::{Node, Parser, Tree};

pub(crate) fn validate_extension_function(
    source: &str,
    function_name: &str,
    expected_arity: usize,
) -> Result<(), String> {
    let tree = parse(source)?;
    let root = tree.root_node();
    if root.has_error() {
        return Err("Lua syntax error".to_string());
    }

    let Some(function) = find_global_function(root, source.as_bytes(), function_name) else {
        return Err(format!("must define Lua function {function_name}"));
    };

    let arity = function_arity(function, source.as_bytes()).ok_or_else(|| {
        format!("failed to inspect Lua function {function_name} parameters")
    })?;
    if arity != expected_arity {
        return Err(format!(
            "Lua function {function_name} expects {expected_arity} args, got {arity}"
        ));
    }

    validate_extension_usage(
        function,
        source.as_bytes(),
        expected_arity,
        function_name == PROCESS_ON_MESSAGE_FN,
    )?;

    Ok(())
}

fn parse(source: &str) -> Result<Tree, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_lua::LANGUAGE.into())
        .map_err(|error| format!("failed to load Lua parser: {error}"))?;
    parser
        .parse(source, None)
        .ok_or_else(|| "failed to parse Lua source".to_string())
}

fn find_global_function<'tree>(
    node: Node<'tree>,
    source: &[u8],
    function_name: &str,
) -> Option<Node<'tree>> {
    if is_named_function(node, source, function_name) || is_assigned_function(node, source, function_name) {
        return function_node(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(function) = find_global_function(child, source, function_name) {
            return Some(function);
        }
    }
    None
}

fn is_named_function(node: Node<'_>, source: &[u8], function_name: &str) -> bool {
    if node.kind() != "function_declaration" {
        return false;
    }
    if node
        .utf8_text(source)
        .unwrap_or_default()
        .trim_start()
        .starts_with("local function")
    {
        return false;
    }
    node_text_contains_named_child(node, source, function_name)
}

fn is_assigned_function(node: Node<'_>, source: &[u8], function_name: &str) -> bool {
    if node.kind() != "assignment_statement" && node.kind() != "variable_declaration" {
        return false;
    }
    let text = node.utf8_text(source).unwrap_or_default();
    text.contains(function_name) && text.contains("function")
}

fn node_text_contains_named_child(node: Node<'_>, source: &[u8], function_name: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() && child.utf8_text(source).ok() == Some(function_name) {
            return true;
        }
        if node_text_contains_named_child(child, source, function_name) {
            return true;
        }
    }
    false
}

fn function_node(node: Node<'_>) -> Option<Node<'_>> {
    if node.kind() == "function_declaration" || node.kind() == "function_definition" {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(function) = function_node(child) {
            return Some(function);
        }
    }
    None
}

fn function_arity(function: Node<'_>, source: &[u8]) -> Option<usize> {
    Some(function_parameters(function, source)?.len())
}

fn function_parameters(function: Node<'_>, source: &[u8]) -> Option<Vec<String>> {
    let parameters = find_first_node_kind(function, "parameters")
        .or_else(|| find_first_node_kind(function, "parameter_list"))?;
    let mut names = Vec::new();
    let mut cursor = parameters.walk();
    for child in parameters.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        match child.kind() {
            "identifier" | "variable_name" => {
                names.push(child.utf8_text(source).ok()?.to_string());
            }
            "vararg_expression" | "..." => names.push("...".to_string()),
            _ => {
                let text = child.utf8_text(source).ok()?.trim().to_string();
                if !text.is_empty() && text != "," {
                    names.push(text);
                }
            }
        }
    }
    Some(names)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticType {
    Unknown,
    Ctx,
    Msg,
    Memory,
    Source,
    Topic,
    Metadata,
    Properties,
    JsonValue,
    EmitFn,
}

#[derive(Debug)]
struct TypeEnv {
    variables: HashMap<String, StaticType>,
    process_function: bool,
}

impl TypeEnv {
    fn new(
        function: Node<'_>,
        source: &[u8],
        expected_arity: usize,
        process_function: bool,
    ) -> Result<Self, String> {
        let params = function_parameters(function, source)
            .ok_or_else(|| "failed to inspect Lua extension function parameters".to_string())?;
        let mut variables = HashMap::new();
        if expected_arity >= 1 {
            variables.insert(params[0].clone(), StaticType::Ctx);
        }
        if expected_arity >= 2 {
            variables.insert(params[1].clone(), StaticType::Msg);
        }
        Ok(Self {
            variables,
            process_function,
        })
    }

    fn get(&self, name: &str) -> StaticType {
        self.variables
            .get(name)
            .copied()
            .unwrap_or(StaticType::Unknown)
    }

    fn set(&mut self, name: String, ty: StaticType) {
        self.variables.insert(name, ty);
    }

    fn contains(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }
}

fn validate_extension_usage(
    function: Node<'_>,
    source: &[u8],
    expected_arity: usize,
    process_function: bool,
) -> Result<(), String> {
    let mut env = TypeEnv::new(function, source, expected_arity, process_function)?;
    analyze_node(function, source, &mut env)
}

fn analyze_node(node: Node<'_>, source: &[u8], env: &mut TypeEnv) -> Result<(), String> {
    match node.kind() {
        "assignment_statement" => {
            analyze_assignment(node, source, env)?;
            return Ok(());
        }
        "variable_declaration" => {
            analyze_variable_declaration(node, source, env)?;
            return Ok(());
        }
        "return_statement" if env.process_function => {
            return Err("on_message must not return".to_string());
        }
        "function_call" => validate_function_call(node, source, env)?,
        "dot_index_expression" => {
            infer_expr_type(node, source, env)?;
        }
        "method_index_expression" => {
            infer_expr_type(node, source, env)?;
        }
        "bracket_index_expression" => {
            infer_expr_type(node, source, env)?;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        analyze_node(child, source, env)?;
    }
    Ok(())
}

fn analyze_assignment(node: Node<'_>, source: &[u8], env: &mut TypeEnv) -> Result<(), String> {
    let variables = find_first_node_kind(node, "variable_list");
    let expressions = find_first_node_kind(node, "expression_list");

    if let Some(expressions) = expressions {
        let mut cursor = expressions.walk();
        for child in expressions.children(&mut cursor) {
            if child.is_named() {
                analyze_node(child, source, env)?;
            }
        }
    }

    let Some(variables) = variables else {
        return Ok(());
    };
    let Some(expressions) = expressions else {
        return Ok(());
    };
    let names = direct_named_children(variables)
        .into_iter()
        .filter(|child| child.kind() == "identifier" || child.kind() == "variable_name")
        .collect::<Vec<_>>();
    let values = direct_named_children(expressions);

    for (name, value) in names.into_iter().zip(values.into_iter()) {
        let name = name.utf8_text(source).unwrap_or_default().to_string();
        if !env.contains(&name) {
            return Err(format!("global assignment is not allowed: {name}"));
        }
        let ty = infer_expr_type(value, source, env)?;
        env.set(name, ty);
    }
    Ok(())
}

fn analyze_variable_declaration(
    node: Node<'_>,
    source: &[u8],
    env: &mut TypeEnv,
) -> Result<(), String> {
    let variables = find_first_node_kind(node, "variable_list");
    let expressions = find_first_node_kind(node, "expression_list");

    if let Some(expressions) = expressions {
        let mut cursor = expressions.walk();
        for child in expressions.children(&mut cursor) {
            if child.is_named() {
                analyze_node(child, source, env)?;
            }
        }
    }

    let Some(variables) = variables else {
        return Ok(());
    };
    let names = direct_named_children(variables)
        .into_iter()
        .filter(|child| child.kind() == "identifier" || child.kind() == "variable_name")
        .collect::<Vec<_>>();
    let values = expressions.map(direct_named_children).unwrap_or_default();

    for (index, name) in names.into_iter().enumerate() {
        let name = name.utf8_text(source).unwrap_or_default().to_string();
        let ty = values
            .get(index)
            .copied()
            .map(|value| infer_expr_type(value, source, env))
            .transpose()?
            .unwrap_or(StaticType::Unknown);
        env.set(name, ty);
    }
    Ok(())
}

fn validate_function_call(node: Node<'_>, source: &[u8], env: &mut TypeEnv) -> Result<(), String> {
    let Some(name) = node.child_by_field_name("name") else {
        return Ok(());
    };
    let ty = infer_expr_type(name, source, env)?;
    if ty == StaticType::EmitFn {
        let args = function_call_arg_count(node);
        if args != 1 {
            return Err(format!("ctx.emit expects 1 arg, got {args}"));
        }
        if env.process_function {
            validate_emit_payload_arg(node, source)?;
        }
    }
    Ok(())
}

fn validate_emit_payload_arg(node: Node<'_>, source: &[u8]) -> Result<(), String> {
    let Some(arg) = function_call_arg_nodes(node).into_iter().next() else {
        return Ok(());
    };
    let text = arg.utf8_text(source).unwrap_or_default().trim();
    match arg.kind() {
        "table_constructor" => {
            if !text.contains('=') {
                return Err("ctx.emit payload must be a JSON object".to_string());
            }
        }
        "string" | "number" | "true" | "false" | "nil" => {
            return Err("ctx.emit payload must be a JSON object".to_string());
        }
        _ => {}
    }
    Ok(())
}

fn function_call_arg_count(node: Node<'_>) -> usize {
    function_call_arg_nodes(node).len()
}

fn function_call_arg_nodes(node: Node<'_>) -> Vec<Node<'_>> {
    let Some(arguments) = find_first_node_kind(node, "arguments") else {
        return Vec::new();
    };
    let mut cursor = arguments.walk();
    arguments
        .children(&mut cursor)
        .filter(|child| child.is_named())
        .collect()
}

fn infer_expr_type(node: Node<'_>, source: &[u8], env: &TypeEnv) -> Result<StaticType, String> {
    match node.kind() {
        "identifier" | "variable_name" => Ok(env.get(node.utf8_text(source).unwrap_or_default())),
        "dot_index_expression" => infer_dot_index_type(node, source, env),
        "method_index_expression" => infer_method_index_type(node, source, env),
        "bracket_index_expression" => infer_bracket_index_type(node, source, env),
        _ => Ok(StaticType::Unknown),
    }
}

fn infer_dot_index_type(node: Node<'_>, source: &[u8], env: &TypeEnv) -> Result<StaticType, String> {
    let Some(table) = node.child_by_field_name("table") else {
        return Ok(StaticType::Unknown);
    };
    let Some(field) = node.child_by_field_name("field") else {
        return Ok(StaticType::Unknown);
    };
    let table_ty = infer_expr_type(table, source, env)?;
    let field = field.utf8_text(source).unwrap_or_default();
    match table_ty {
        StaticType::Ctx => match field {
            field if field == script_field::<Context>("memory") => Ok(StaticType::Memory),
            field if field == script_field::<Context>("source") => Ok(StaticType::Source),
            field if field == script_field::<Context>("emit") => Ok(StaticType::EmitFn),
            _ => Err(format!("invalid ctx field: {field}")),
        },
        StaticType::Msg => match field {
            field if field == script_field::<Message>("source") => Ok(StaticType::Source),
            field if field == script_field::<Message>("topic") => Ok(StaticType::Topic),
            field if field == script_field::<Message>("payload") => Ok(StaticType::JsonValue),
            field if field == script_field::<Message>("raw_payload") => Ok(StaticType::JsonValue),
            field if field == script_field::<Message>("metadata") => Ok(StaticType::Metadata),
            field if field == script_field::<Message>("properties") => Ok(StaticType::Properties),
            _ => Err(format!("invalid msg field: {field}")),
        },
        StaticType::Source => match field {
            field if SourceContext::FIELDS.contains(&field) => Ok(StaticType::JsonValue),
            _ => Err(format!("invalid source field: {field}")),
        },
        StaticType::Topic => match field {
            field if Topic::FIELDS.contains(&field) => Ok(StaticType::JsonValue),
            _ => Err(format!("invalid topic field: {field}")),
        },
        StaticType::Metadata => match field {
            field if MqttMetadata::FIELDS.contains(&field) => Ok(StaticType::JsonValue),
            _ => Err(format!("invalid metadata field: {field}")),
        },
        StaticType::Memory => match field {
            field if MemoryView::METHODS.contains(&field) => Ok(StaticType::Unknown),
            _ => Err(format!(
                "invalid memory field: {field}; use memory.get/set/delete"
            )),
        },
        StaticType::Properties | StaticType::JsonValue => Ok(StaticType::JsonValue),
        StaticType::Unknown | StaticType::EmitFn => Ok(StaticType::Unknown),
    }
}

fn infer_method_index_type(node: Node<'_>, source: &[u8], env: &TypeEnv) -> Result<StaticType, String> {
    let Some(table) = node.child_by_field_name("table") else {
        return Ok(StaticType::Unknown);
    };
    let Some(method) = node.child_by_field_name("method") else {
        return Ok(StaticType::Unknown);
    };
    let table_ty = infer_expr_type(table, source, env)?;
    let method = method.utf8_text(source).unwrap_or_default();
    match table_ty {
        StaticType::Memory => Err(format!("invalid memory method: {method}; use memory.{method}")),
        StaticType::Unknown => Ok(StaticType::Unknown),
        _ => Err(format!("invalid method call on known Tenon type: {method}")),
    }
}

fn script_field<T: ScriptApi>(field: &'static str) -> &'static str {
    debug_assert!(T::FIELDS.contains(&field));
    field
}

fn infer_bracket_index_type(node: Node<'_>, source: &[u8], env: &TypeEnv) -> Result<StaticType, String> {
    let Some(table) = node.child_by_field_name("table") else {
        return Ok(StaticType::Unknown);
    };
    match infer_expr_type(table, source, env)? {
        StaticType::Topic | StaticType::Properties | StaticType::JsonValue => Ok(StaticType::JsonValue),
        StaticType::Unknown => Ok(StaticType::Unknown),
        known => Err(format!("invalid bracket access on {known:?}")),
    }
}

fn direct_named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| child.is_named())
        .collect()
}

fn find_first_node_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_first_node_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tenon_extension::{AUTH_CREDENTIALS_FN, PROCESS_ON_MESSAGE_FN};

    #[test]
    fn validates_named_extension_functions() {
        validate_extension_function(
            "function credentials(ctx) return { type = 'username-password', username = 'u', password = 'p' } end",
            AUTH_CREDENTIALS_FN,
            1,
        )
        .expect("auth function");

        validate_extension_function(
            "function on_message(ctx, msg) ctx.emit(msg.payload) end",
            PROCESS_ON_MESSAGE_FN,
            2,
        )
        .expect("process function");
    }

    #[test]
    fn validates_credentials_username_password_shape() {
        validate_extension_function(
            r#"
function credentials(ctx)
  return {
    type = "username-password",
    username = "dev",
    password = "dev"
  }
end
"#,
            AUTH_CREDENTIALS_FN,
            1,
        )
        .expect("username/password credentials");
    }

    #[test]
    fn validates_credentials_bearer_token_shape() {
        validate_extension_function(
            r#"
function credentials(ctx)
  local token = "token-" .. "value"
  return {
    type = "bearer-token",
    token = token
  }
end
"#,
            AUTH_CREDENTIALS_FN,
            1,
        )
        .expect("bearer-token credentials");
    }

    #[test]
    fn validates_credentials_client_certificate_shape() {
        validate_extension_function(
            r#"
function credentials(ctx)
  return {
    type = "client-certificate",
    cert_path = "/etc/tenon/certs/client.crt",
    key_path = "/etc/tenon/certs/client.key",
    ca_path = "/etc/tenon/certs/ca.crt"
  }
end
"#,
            AUTH_CREDENTIALS_FN,
            1,
        )
        .expect("client certificate credentials");
    }

    #[test]
    fn validates_credentials_custom_dynamic_shape() {
        validate_extension_function(
            r#"
function credentials(ctx)
  local ts = "1770000000"
  local signature = "sig-" .. ts
  return {
    type = "custom",
    username = "device",
    password = signature,
    properties = {
      timestamp = ts,
      signature = signature
    }
  }
end
"#,
            AUTH_CREDENTIALS_FN,
            1,
        )
        .expect("custom dynamic credentials");
    }

    #[test]
    fn validates_assigned_extension_function() {
        validate_extension_function(
            "on_message = function(ctx, msg) ctx.emit(msg.payload) end",
            PROCESS_ON_MESSAGE_FN,
            2,
        )
        .expect("assigned process function");
    }

    #[test]
    fn rejects_local_extension_function() {
        let error = validate_extension_function(
            "local function on_message(ctx, msg) end",
            PROCESS_ON_MESSAGE_FN,
            2,
        )
        .expect_err("local function is not global extension entry");
        assert!(error.contains(PROCESS_ON_MESSAGE_FN));
    }

    #[test]
    fn rejects_invalid_lua_source() {
        let error = validate_extension_function("function on_message(ctx, msg)", PROCESS_ON_MESSAGE_FN, 2)
            .expect_err("syntax error");
        assert!(error.contains("syntax error"));
    }

    #[test]
    fn rejects_missing_extension_function() {
        let error = validate_extension_function("function other(ctx, msg) end", PROCESS_ON_MESSAGE_FN, 2)
            .expect_err("missing function");
        assert!(error.contains(PROCESS_ON_MESSAGE_FN));
    }

    #[test]
    fn rejects_wrong_extension_arity() {
        let error = validate_extension_function("function on_message(ctx) end", PROCESS_ON_MESSAGE_FN, 2)
            .expect_err("wrong arity");
        assert!(error.contains("expects 2 args"));
    }

    #[test]
    fn validates_ctx_msg_usage_with_aliases() {
        validate_extension_function(
            r#"
function on_message(c, m)
  local memory = c.memory
  local payload = m.payload
  local topic = m.topic
  local source = m.source
  local metadata = m.metadata
  local properties = m.properties
  local temp = payload.temp
  local first = topic[1]
  local raw = topic.raw
  local name = source.name
  local qos = metadata.qos
  local prop = properties["x"]
  memory.set("last", temp)
  c.emit({ temp = temp, first = first, raw = raw, name = name, qos = qos, prop = prop })
end
"#,
            PROCESS_ON_MESSAGE_FN,
            2,
        )
        .expect("valid Tenon extension usage");
    }

    #[test]
    fn rejects_invalid_ctx_field_after_aliasing() {
        let error = validate_extension_function(
            r#"
function on_message(ctx, msg)
  local a = ctx
  local x = a.device
end
"#,
            PROCESS_ON_MESSAGE_FN,
            2,
        )
        .expect_err("invalid ctx field");
        assert!(error.contains("invalid ctx field: device"));
    }

    #[test]
    fn rejects_invalid_msg_field_after_aliasing() {
        let error = validate_extension_function(
            r#"
function on_message(ctx, msg)
  local m = msg
  local x = m.device
end
"#,
            PROCESS_ON_MESSAGE_FN,
            2,
        )
        .expect_err("invalid msg field");
        assert!(error.contains("invalid msg field: device"));
    }

    #[test]
    fn rejects_invalid_memory_method() {
        let error = validate_extension_function(
            r#"
function on_message(ctx, msg)
  local m = ctx.memory
  m:put("a", "b")
end
"#,
            PROCESS_ON_MESSAGE_FN,
            2,
        )
        .expect_err("invalid memory method");
        assert!(error.contains("invalid memory method: put"));
    }

    #[test]
    fn rejects_wrong_emit_arity() {
        let error = validate_extension_function(
            "function on_message(ctx, msg) ctx.emit('a', msg.payload) end",
            PROCESS_ON_MESSAGE_FN,
            2,
        )
        .expect_err("wrong emit arity");
        assert!(error.contains("ctx.emit expects 1 arg"));
    }

    #[test]
    fn rejects_on_message_return() {
        let error = validate_extension_function(
            "function on_message(ctx, msg) return { temp = msg.payload.temp } end",
            PROCESS_ON_MESSAGE_FN,
            2,
        )
        .expect_err("on_message return");
        assert!(error.contains("on_message must not return"));
    }

    #[test]
    fn rejects_global_assignment() {
        let error = validate_extension_function(
            "function on_message(ctx, msg) count = 1 end ",
            PROCESS_ON_MESSAGE_FN,
            2,
        )
        .expect_err("global assignment");
        assert!(error.contains("global assignment is not allowed: count"));
    }

    #[test]
    fn rejects_obvious_non_object_emit_payload() {
        let error = validate_extension_function(
            "function on_message(ctx, msg) ctx.emit({1, 2, 3}) end",
            PROCESS_ON_MESSAGE_FN,
            2,
        )
        .expect_err("non-object emit payload");
        assert!(error.contains("ctx.emit payload must be a JSON object"));
    }
}
