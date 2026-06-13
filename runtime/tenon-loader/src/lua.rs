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
    let parameters = find_first_node_kind(function, "parameters")
        .or_else(|| find_first_node_kind(function, "parameter_list"))?;

    let mut count = 0;
    let mut cursor = parameters.walk();
    for child in parameters.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        match child.kind() {
            "identifier" | "variable_name" => count += 1,
            "vararg_expression" | "..." => count += 1,
            _ => {
                let text = child.utf8_text(source).unwrap_or_default();
                if !text.trim().is_empty() && text.trim() != "," {
                    count += 1;
                }
            }
        }
    }
    Some(count)
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
            "function credentials(ctx) return { username = 'u', password = 'p' } end",
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
}
