use std::collections::{BTreeSet, HashMap};
use tenon_extension::{
    Context, MemoryView, Message, MqttMetadata, PROCESS_ON_MESSAGE_FN, ScriptApi, SourceContext,
    Topic,
};
use tenon_message::{
    daemon::v1::json_path_segment,
    plan::{
        AccessMode, ByteRange, JsonAccess, JsonPath, JsonPathSegment, MessageAccessPlan,
        MetadataAccess, PropertiesAccess, RawPayloadAccess, SourceAccess, TopicAccess,
    },
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

pub(crate) fn analyze_process_access_plan(source: &str) -> Result<MessageAccessPlan, String> {
    let tree = parse(source)?;
    let root = tree.root_node();
    // the source script has been parsed twiece for a single loading
    if root.has_error() {
        return Err("Lua syntax error".to_string());
    }

    let Some(function) = find_global_function(root, source.as_bytes(), PROCESS_ON_MESSAGE_FN) else {
        return Err(format!("must define Lua function {PROCESS_ON_MESSAGE_FN}"));
    };

    let arity = function_arity(function, source.as_bytes()).ok_or_else(|| {
        format!("failed to inspect Lua function {PROCESS_ON_MESSAGE_FN} parameters")
    })?;
    if arity != 2 {
        return Err(format!(
            "Lua function {PROCESS_ON_MESSAGE_FN} expects 2 args, got {arity}"
        ));
    }

    let mut env = TypeEnv::new(function, source.as_bytes(), 2, true)?;
    analyze_node(function, source.as_bytes(), &mut env)?;
    Ok(env.access.into_plan())
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
    TopicLevels,
    RawPayload,
    Metadata,
    Properties,
    JsonValue,
    EmitFn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StaticValue {
    Unknown,
    String(String),
    Int(i64),
}

impl Default for StaticValue {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Binding {
    ty: StaticType,
    value: StaticValue,
    access: AccessBinding,
}

impl Binding {
    fn unknown() -> Self {
        Self {
            ty: StaticType::Unknown,
            value: StaticValue::Unknown,
            access: AccessBinding::None,
        }
    }

    fn typed(ty: StaticType) -> Self {
        Self {
            ty,
            value: StaticValue::Unknown,
            access: AccessBinding::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AccessBinding {
    None,
    Source,
    Topic,
    TopicLevels,
    Payload(Vec<AccessPathSegment>),
    RawPayload,
    Metadata,
    Properties,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum AccessPathSegment {
    Field(String),
    Index(u32),
}

#[derive(Debug, Default)]
struct AccessPlanBuilder {
    source_full: bool,
    source_name: bool,
    source_version: bool,
    topic_full: bool,
    topic_raw: bool,
    topic_levels: bool,
    topic_level_indexes: BTreeSet<u32>,
    payload_full: bool,
    payload_paths: BTreeSet<Vec<AccessPathSegment>>,
    raw_payload_full: bool,
    raw_payload_ranges: BTreeSet<(u32, u32)>,
    metadata_full: bool,
    metadata_pkid: bool,
    metadata_qos: bool,
    metadata_retain: bool,
    metadata_dup: bool,
    properties_full: bool,
    property_keys: BTreeSet<String>,
}

impl AccessPlanBuilder {
    fn source_mode(&self) -> AccessMode {
        if self.source_full {
            AccessMode::Full
        } else if self.source_name || self.source_version {
            AccessMode::Selective
        } else {
            AccessMode::None
        }
    }

    fn topic_mode(&self) -> AccessMode {
        if self.topic_full {
            AccessMode::Full
        } else if self.topic_raw || self.topic_levels || !self.topic_level_indexes.is_empty() {
            AccessMode::Selective
        } else {
            AccessMode::None
        }
    }

    fn payload_mode(&self) -> AccessMode {
        if self.payload_full {
            AccessMode::Full
        } else if !self.payload_paths.is_empty() {
            AccessMode::Selective
        } else {
            AccessMode::None
        }
    }

    fn raw_payload_mode(&self) -> AccessMode {
        if self.raw_payload_full {
            AccessMode::Full
        } else if !self.raw_payload_ranges.is_empty() {
            AccessMode::Selective
        } else {
            AccessMode::None
        }
    }

    fn metadata_mode(&self) -> AccessMode {
        if self.metadata_full {
            AccessMode::Full
        } else if self.metadata_pkid || self.metadata_qos || self.metadata_retain || self.metadata_dup {
            AccessMode::Selective
        } else {
            AccessMode::None
        }
    }

    fn properties_mode(&self) -> AccessMode {
        if self.properties_full {
            AccessMode::Full
        } else if !self.property_keys.is_empty() {
            AccessMode::Selective
        } else {
            AccessMode::None
        }
    }

    fn record_source_field(&mut self, field: &str) {
        match field {
            "name" => self.source_name = true,
            "version" => self.source_version = true,
            _ => self.source_full = true,
        }
    }

    fn record_topic_field(&mut self, field: &str) {
        match field {
            "raw" => self.topic_raw = true,
            "levels" => self.topic_levels = true,
            _ => self.topic_full = true,
        }
    }

    fn record_topic_level(&mut self, index: u32) {
        self.topic_level_indexes.insert(index);
    }

    fn record_payload_path(&mut self, path: &[AccessPathSegment]) {
        if path.is_empty() {
            self.payload_full = true;
        } else if !self.payload_full {
            self.payload_paths.insert(path.to_vec());
        }
    }

    fn record_raw_payload_index(&mut self, lua_index: u32) {
        let offset = lua_index.saturating_sub(1);
        self.raw_payload_ranges.insert((offset, 1));
    }

    fn record_metadata_field(&mut self, field: &str) {
        match field {
            "pkid" => self.metadata_pkid = true,
            "qos" => self.metadata_qos = true,
            "retain" => self.metadata_retain = true,
            "dup" => self.metadata_dup = true,
            _ => self.metadata_full = true,
        }
    }

    fn record_property_key(&mut self, key: &str) {
        if !self.properties_full {
            self.property_keys.insert(key.to_string());
        }
    }

    fn into_plan(self) -> MessageAccessPlan {
        let source_mode = self.source_mode();
        let topic_mode = self.topic_mode();
        let payload_mode = self.payload_mode();
        let raw_payload_mode = self.raw_payload_mode();
        let metadata_mode = self.metadata_mode();
        let properties_mode = self.properties_mode();
        MessageAccessPlan {
            source: Some(SourceAccess {
                mode: source_mode as i32,
                name: self.source_name,
                version: self.source_version,
            }),
            topic: Some(TopicAccess {
                mode: topic_mode as i32,
                raw: self.topic_raw,
                levels: self.topic_levels,
                level_indexes: self.topic_level_indexes.into_iter().collect(),
            }),
            payload: Some(JsonAccess {
                mode: payload_mode as i32,
                paths: self
                    .payload_paths
                    .into_iter()
                    .map(|segments| JsonPath {
                        segments: segments.into_iter().map(json_path_segment).collect(),
                    })
                    .collect(),
            }),
            raw_payload: Some(RawPayloadAccess {
                mode: raw_payload_mode as i32,
                ranges: self
                    .raw_payload_ranges
                    .into_iter()
                    .map(|(offset, length)| ByteRange { offset, length })
                    .collect(),
            }),
            metadata: Some(MetadataAccess {
                mode: metadata_mode as i32,
                pkid: self.metadata_pkid,
                qos: self.metadata_qos,
                retain: self.metadata_retain,
                dup: self.metadata_dup,
            }),
            properties: Some(PropertiesAccess {
                mode: properties_mode as i32,
                keys: self.property_keys.into_iter().collect(),
            }),
        }
    }
}

fn json_path_segment(segment: AccessPathSegment) -> JsonPathSegment {
    let kind = match segment {
        AccessPathSegment::Field(field) => json_path_segment::Kind::Field(field),
        AccessPathSegment::Index(index) => json_path_segment::Kind::Index(index),
    };
    JsonPathSegment { kind: Some(kind) }
}

#[derive(Debug)]
struct TypeEnv {
    variables: HashMap<String, Binding>,
    process_function: bool,
    access: AccessPlanBuilder,
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
            variables.insert(params[0].clone(), Binding::typed(StaticType::Ctx));
        }
        if expected_arity >= 2 {
            variables.insert(params[1].clone(), Binding::typed(StaticType::Msg));
        }
        Ok(Self {
            variables,
            process_function,
            access: AccessPlanBuilder::default(),
        })
    }

    fn get(&self, name: &str) -> StaticType {
        self.variables
            .get(name)
            .map(|binding| binding.ty)
            .unwrap_or(StaticType::Unknown)
    }

    fn value(&self, name: &str) -> StaticValue {
        self.variables
            .get(name)
            .map(|binding| binding.value.clone())
            .unwrap_or(StaticValue::Unknown)
    }

    fn access_binding(&self, name: &str) -> AccessBinding {
        self.variables
            .get(name)
            .map(|binding| binding.access.clone())
            .unwrap_or(AccessBinding::None)
    }

    fn set(&mut self, name: String, binding: Binding) {
        self.variables.insert(name, binding);
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
        let binding = infer_expr_binding(value, source, env)?;
        env.set(name, binding);
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
        let binding = values
            .get(index)
            .copied()
            .map(|value| infer_expr_binding(value, source, env))
            .transpose()?
            .unwrap_or_else(Binding::unknown);
        env.set(name, binding);
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

fn infer_expr_type(node: Node<'_>, source: &[u8], env: &mut TypeEnv) -> Result<StaticType, String> {
    Ok(infer_expr_binding(node, source, env)?.ty)
}

fn infer_expr_binding(node: Node<'_>, source: &[u8], env: &mut TypeEnv) -> Result<Binding, String> {
    let value = infer_static_value(node, source, env);
    match node.kind() {
        "identifier" | "variable_name" => {
            let name = node.utf8_text(source).unwrap_or_default();
            Ok(Binding {
                ty: env.get(name),
                value: env.value(name),
                access: env.access_binding(name),
            })
        }
        "dot_index_expression" => {
            let (ty, access) = infer_dot_index_type(node, source, env)?;
            Ok(Binding { ty, value, access })
        }
        "method_index_expression" => Ok(Binding {
            ty: infer_method_index_type(node, source, env)?,
            value,
            access: AccessBinding::None,
        }),
        "bracket_index_expression" => {
            let (ty, access) = infer_bracket_index_type(node, source, env)?;
            Ok(Binding { ty, value, access })
        }
        _ => Ok(Binding {
            ty: StaticType::Unknown,
            value,
            access: AccessBinding::None,
        }),
    }
}

fn infer_static_value(node: Node<'_>, source: &[u8], env: &mut TypeEnv) -> StaticValue {
    match node.kind() {
        "identifier" | "variable_name" => env.value(node.utf8_text(source).unwrap_or_default()),
        "string" => string_literal_value(node, source)
            .map(StaticValue::String)
            .unwrap_or(StaticValue::Unknown),
        "number" => node
            .utf8_text(source)
            .ok()
            .and_then(|text| text.parse::<i64>().ok())
            .map(StaticValue::Int)
            .unwrap_or(StaticValue::Unknown),
        _ => StaticValue::Unknown,
    }
}

fn string_literal_value(node: Node<'_>, source: &[u8]) -> Option<String> {
    let text = node.utf8_text(source).ok()?.trim();
    if text.len() < 2 {
        return None;
    }
    let bytes = text.as_bytes();
    if !((bytes[0] == b'"' && bytes[text.len() - 1] == b'"')
        || (bytes[0] == b'\'' && bytes[text.len() - 1] == b'\''))
    {
        return None;
    }
    Some(text[1..text.len() - 1].to_string())
}

fn infer_dot_index_type(
    node: Node<'_>,
    source: &[u8],
    env: &mut TypeEnv,
) -> Result<(StaticType, AccessBinding), String> {
    let Some(table) = node.child_by_field_name("table") else {
        return Ok((StaticType::Unknown, AccessBinding::None));
    };
    let Some(field) = node.child_by_field_name("field") else {
        return Ok((StaticType::Unknown, AccessBinding::None));
    };
    let table_binding = infer_expr_binding(table, source, env)?;
    let field = field.utf8_text(source).unwrap_or_default();
    match table_binding.ty {
        StaticType::Ctx => match field {
            field if field == script_field::<Context>("memory") => Ok((StaticType::Memory, AccessBinding::None)),
            field if field == script_field::<Context>("source") => Ok((StaticType::Source, AccessBinding::Source)),
            field if field == script_field::<Context>("emit") => Ok((StaticType::EmitFn, AccessBinding::None)),
            _ => Err(format!("invalid ctx field: {field}")),
        },
        StaticType::Msg => match field {
            field if field == script_field::<Message>("source") => Ok((StaticType::Source, AccessBinding::Source)),
            field if field == script_field::<Message>("topic") => Ok((StaticType::Topic, AccessBinding::Topic)),
            field if field == script_field::<Message>("payload") => {
                Ok((StaticType::JsonValue, AccessBinding::Payload(Vec::new())))
            }
            field if field == script_field::<Message>("raw_payload") => Ok((StaticType::RawPayload, AccessBinding::RawPayload)),
            field if field == script_field::<Message>("metadata") => Ok((StaticType::Metadata, AccessBinding::Metadata)),
            field if field == script_field::<Message>("properties") => Ok((StaticType::Properties, AccessBinding::Properties)),
            _ => Err(format!("invalid msg field: {field}")),
        },
        StaticType::Source => match field {
            field if SourceContext::FIELDS.contains(&field) => {
                env.access.record_source_field(field);
                Ok((StaticType::JsonValue, AccessBinding::None))
            }
            _ => Err(format!("invalid source field: {field}")),
        },
        StaticType::Topic => match field {
            "levels" => Ok((StaticType::TopicLevels, AccessBinding::TopicLevels)),
            field if Topic::FIELDS.contains(&field) => {
                env.access.record_topic_field(field);
                Ok((StaticType::JsonValue, AccessBinding::None))
            }
            _ => Err(format!("invalid topic field: {field}")),
        },
        StaticType::Metadata => match field {
            field if MqttMetadata::FIELDS.contains(&field) => {
                env.access.record_metadata_field(field);
                Ok((StaticType::JsonValue, AccessBinding::None))
            }
            _ => Err(format!("invalid metadata field: {field}")),
        },
        StaticType::Properties => {
            env.access.record_property_key(field);
            Ok((StaticType::JsonValue, AccessBinding::None))
        }
        StaticType::JsonValue => match table_binding.access {
            AccessBinding::Payload(mut path) => {
                path.push(AccessPathSegment::Field(field.to_string()));
                env.access.record_payload_path(&path);
                Ok((StaticType::JsonValue, AccessBinding::Payload(path)))
            }
            _ => Ok((StaticType::JsonValue, AccessBinding::None)),
        },
        StaticType::Memory => match field {
            field if MemoryView::METHODS.contains(&field) => Ok((StaticType::Unknown, AccessBinding::None)),
            _ => Err(format!(
                "invalid memory field: {field}; use memory.get/set/delete"
            )),
        },
        StaticType::RawPayload | StaticType::TopicLevels => {
            Err(format!("invalid dot access on {:?}", table_binding.ty))
        }
        StaticType::Unknown | StaticType::EmitFn => Ok((StaticType::Unknown, AccessBinding::None)),
    }
}

fn infer_method_index_type(node: Node<'_>, source: &[u8], env: &mut TypeEnv) -> Result<StaticType, String> {
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

fn infer_bracket_index_type(
    node: Node<'_>,
    source: &[u8],
    env: &mut TypeEnv,
) -> Result<(StaticType, AccessBinding), String> {
    let Some(table) = node.child_by_field_name("table") else {
        return Ok((StaticType::Unknown, AccessBinding::None));
    };
    let table_binding = infer_expr_binding(table, source, env)?;
    let access = infer_bracket_access(node, source, env);
    match table_binding.ty {
        StaticType::Topic | StaticType::TopicLevels => {
            if let Some(StaticValue::Int(index)) = access {
                if let Ok(index) = u32::try_from(index) {
                    env.access.record_topic_level(index);
                } else {
                    env.access.topic_levels = true;
                }
            } else {
                env.access.topic_levels = true;
            }
            Ok((StaticType::JsonValue, AccessBinding::None))
        }
        StaticType::Properties => {
            if let Some(StaticValue::String(key)) = access {
                env.access.record_property_key(&key);
            } else {
                env.access.properties_full = true;
            }
            Ok((StaticType::JsonValue, AccessBinding::None))
        }
        StaticType::JsonValue => match table_binding.access {
            AccessBinding::Payload(mut path) => {
                match access {
                    Some(StaticValue::String(field)) => {
                        path.push(AccessPathSegment::Field(field));
                        env.access.record_payload_path(&path);
                    }
                    Some(StaticValue::Int(index)) => {
                        if let Ok(index) = u32::try_from(index) {
                            path.push(AccessPathSegment::Index(index));
                            env.access.record_payload_path(&path);
                        } else {
                            env.access.payload_full = true;
                        }
                    }
                    _ => env.access.payload_full = true,
                }
                Ok((StaticType::JsonValue, AccessBinding::Payload(path)))
            }
            _ => Ok((StaticType::JsonValue, AccessBinding::None)),
        },
        StaticType::RawPayload => {
            if let Some(StaticValue::Int(index)) = access {
                if let Ok(index) = u32::try_from(index) {
                    env.access.record_raw_payload_index(index);
                } else {
                    env.access.raw_payload_full = true;
                }
            } else {
                env.access.raw_payload_full = true;
            }
            Ok((StaticType::JsonValue, AccessBinding::None))
        }
        StaticType::Unknown => Ok((StaticType::Unknown, AccessBinding::None)),
        known => Err(format!("invalid bracket access on {known:?}")),
    }
}

fn infer_bracket_access(
    node: Node<'_>,
    source: &[u8],
    env: &mut TypeEnv,
) -> Option<StaticValue> {
    let key = node
        .child_by_field_name("field")
        .or_else(|| node.child_by_field_name("index"))
        .or_else(|| direct_named_children(node).into_iter().last())?;
    match infer_static_value(key, source, env) {
        StaticValue::Unknown => None,
        value => Some(value),
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

    fn path_fields(path: &JsonPath) -> Vec<&str> {
        path.segments
            .iter()
            .map(|segment| match segment.kind.as_ref().expect("path segment") {
                json_path_segment::Kind::Field(field) => field.as_str(),
                json_path_segment::Kind::Index(_) => "<index>",
            })
            .collect()
    }

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
    fn propagates_literal_constants_for_static_bracket_access() {
        let source = r#"
function on_message(ctx, msg)
  local payload = msg.payload
  local key = "temp"
  local topic = msg.topic
  local index = 2
  local temp = payload[key]
  local device = topic[index]
  ctx.emit({ temp = temp, device = device })
end
"#;
        let tree = parse(source).expect("tree");
        let function = find_global_function(tree.root_node(), source.as_bytes(), PROCESS_ON_MESSAGE_FN)
            .expect("function");
        let mut env = TypeEnv::new(function, source.as_bytes(), 2, true).expect("env");
        analyze_node(function, source.as_bytes(), &mut env).expect("analysis");

        assert_eq!(env.value("key"), StaticValue::String("temp".to_string()));
        assert_eq!(env.value("index"), StaticValue::Int(2));
        assert_eq!(env.get("temp"), StaticType::JsonValue);
        assert_eq!(env.get("device"), StaticType::JsonValue);
    }

    #[test]
    fn keeps_runtime_dependent_bracket_access_dynamic() {
        let source = r#"
function on_message(ctx, msg)
  local payload = msg.payload
  local key = ctx.memory.get("field")
  local temp = payload[key]
  ctx.emit({ temp = temp })
end
"#;
        let tree = parse(source).expect("tree");
        let function = find_global_function(tree.root_node(), source.as_bytes(), PROCESS_ON_MESSAGE_FN)
            .expect("function");
        let mut env = TypeEnv::new(function, source.as_bytes(), 2, true).expect("env");
        analyze_node(function, source.as_bytes(), &mut env).expect("analysis");

        assert_eq!(env.value("key"), StaticValue::Unknown);
        assert_eq!(env.get("temp"), StaticType::JsonValue);
    }

    #[test]
    fn generates_static_message_access_plan() {
        let plan = analyze_process_access_plan(
            r#"
function on_message(ctx, msg)
  local payload = msg.payload
  local key = "temp"
  local level = 2
  local raw_index = 1
  ctx.emit({
    source = msg.source.version,
    topic = msg.topic.raw,
    device = msg.topic.levels[level],
    temp = payload[key],
    first = payload.values[1],
    byte = msg.raw_payload[raw_index],
    dup = msg.metadata.dup,
    site = msg.properties["site"]
  })
end
"#,
        )
        .expect("access plan");

        let source = plan.source.as_ref().expect("source access");
        assert_eq!(source.mode, AccessMode::Selective as i32);
        assert!(!source.name);
        assert!(source.version);

        let topic = plan.topic.as_ref().expect("topic access");
        assert_eq!(topic.mode, AccessMode::Selective as i32);
        assert!(topic.raw);
        assert_eq!(topic.level_indexes, vec![2]);

        let payload = plan.payload.as_ref().expect("payload access");
        assert_eq!(payload.mode, AccessMode::Selective as i32);
        let paths = payload.paths.iter().map(path_fields).collect::<Vec<_>>();
        assert_eq!(paths, vec![vec!["temp"], vec!["values"], vec!["values", "<index>"]]);

        let raw_payload = plan.raw_payload.as_ref().expect("raw payload access");
        assert_eq!(raw_payload.mode, AccessMode::Selective as i32);
        assert_eq!(raw_payload.ranges.len(), 1);
        assert_eq!(raw_payload.ranges[0].offset, 0);
        assert_eq!(raw_payload.ranges[0].length, 1);

        let metadata = plan.metadata.as_ref().expect("metadata access");
        assert_eq!(metadata.mode, AccessMode::Selective as i32);
        assert!(metadata.dup);
        assert!(!metadata.qos);

        let properties = plan.properties.as_ref().expect("properties access");
        assert_eq!(properties.mode, AccessMode::Selective as i32);
        assert_eq!(properties.keys, vec!["site"]);
    }

    #[test]
    fn marks_dynamic_message_access_as_full() {
        let plan = analyze_process_access_plan(
            r#"
function on_message(ctx, msg)
  local key = ctx.memory.get("field")
  ctx.emit({
    payload = msg.payload[key],
    raw = msg.raw_payload[key],
    prop = msg.properties[key],
    level = msg.topic.levels[key]
  })
end
"#,
        )
        .expect("access plan");

        assert_eq!(
            plan.payload.as_ref().expect("payload access").mode,
            AccessMode::Full as i32
        );
        assert_eq!(
            plan.raw_payload.as_ref().expect("raw payload access").mode,
            AccessMode::Full as i32
        );
        assert_eq!(
            plan.properties.as_ref().expect("properties access").mode,
            AccessMode::Full as i32
        );
        let topic = plan.topic.as_ref().expect("topic access");
        assert_eq!(topic.mode, AccessMode::Selective as i32);
        assert!(topic.levels);
        assert!(topic.level_indexes.is_empty());
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
