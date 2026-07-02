use crate::{WorkerError, WorkerResult};
use mlua::{Function, HookTriggers, Lua, LuaOptions, Scope, StdLib, Table, Value, VmState};
use serde_json::{Map, Number};
use std::cell::RefCell;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tenon_extension::{
    Context, ExtensionValue, InvocationOutcome, Message, PROCESS_ON_MESSAGE_FN,
};
use tenon_message::{
    daemon::v1::{json_path_segment, JsonPathSegment},
    plan::{
        AccessMode, JsonAccess, MessageAccessPlan, MetadataAccess, ProcessPlan,
        PropertiesAccess, RawPayloadAccess, SourceAccess, TopicAccess,
    },
};

pub trait Processor: Send + 'static {
    fn process(&mut self, message: &Message) -> WorkerResult<InvocationOutcome>;

    fn into_context(self: Box<Self>) -> Context;
}

#[derive(Debug)]
pub struct LuaProcessor {
    lua: Lua,
    on_message: Function,
    context: Context,
    access_plan: MessageAccessPlan,
    instruction_budget: Arc<AtomicU64>,
}

impl LuaProcessor {
    pub fn new(process: ProcessPlan, context: Context) -> WorkerResult<Self> {
        let lua = Lua::new_with(safe_lua_libs(), LuaOptions::default()).map_err(lua_error)?;
        let instruction_budget = Arc::new(AtomicU64::new(0));
        install_instruction_budget_hook(&lua, Arc::clone(&instruction_budget));
        lua.load(&process.source)
            .exec()
            .map_err(lua_error)?;
        let on_message = lua
            .globals()
            .get(PROCESS_ON_MESSAGE_FN)
            .map_err(lua_error)?;
        let access_plan = process.access_plan.unwrap_or_else(full_message_access_plan);
        Ok(Self {
            lua,
            on_message,
            context,
            access_plan,
            instruction_budget,
        })
    }
}

impl Processor for LuaProcessor {
    fn process(&mut self, message: &Message) -> WorkerResult<InvocationOutcome> {
        let context = RefCell::new(&mut self.context);
        self.instruction_budget
            .store(DEFAULT_LUA_INSTRUCTION_BUDGET, Ordering::Relaxed);
        self.lua
            .scope(|scope| {
                let ctx = create_context_table(&self.lua, scope, &context)?;
                let msg = create_message_table(&self.lua, message, &self.access_plan)?;
                self.on_message.call::<Value>((ctx, msg))
            })
            .and_then(validate_on_message_return)
            .map_err(lua_error)?;
        Ok(self.context.drain_outcome())
    }

    fn into_context(self: Box<Self>) -> Context {
        let Self {
            lua,
            on_message,
            context,
            access_plan: _,
            instruction_budget: _,
        } = *self;
        drop(on_message);
        drop(lua);
        context
    }
}

pub fn processor_from_plan(
    process: Option<ProcessPlan>,
    context: Context,
) -> WorkerResult<Box<dyn Processor>> {
    let process_plan = process.ok_or_else(|| WorkerError::pipeline("process plan is missing"))?;
    Ok(Box::new(LuaProcessor::new(process_plan, context)?))
}

fn create_context_table<'scope, 'env>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    context: &'scope RefCell<&'env mut Context>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let source = {
        let context = context.borrow();
        context.source().clone()
    };
    table.set("source", create_source_table(lua, &source)?)?;
    table.set("memory", create_memory_table(lua, scope, context)?)?;
    let emit = scope
        .create_function(move |_, payload: Value| {
            let payload = lua_value_to_emit_payload(payload)?;
            let mut context = context.borrow_mut();
            context.emit(payload).map_err(mlua::Error::external)
        })
        ?;
    table.set("emit", emit)?;
    Ok(table)
}

fn create_memory_table<'scope, 'env>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    context: &'scope RefCell<&'env mut Context>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let get = scope
        .create_function(move |lua, key: String| {
            let context = context.borrow();
            match context.memory_get(&key).map_err(mlua::Error::external)? {
                Some(value) => json_to_lua_value(lua, &value),
                None => Ok(Value::Nil),
            }
        })
        ?;
    table.set("get", get)?;
    let set = scope
        .create_function(move |_, (key, value): (String, Value)| {
            let value = lua_value_to_json(value)?;
            let mut context = context.borrow_mut();
            context.memory_set(key, value).map_err(mlua::Error::external)
        })
        ?;
    table.set("set", set)?;
    let delete = scope
        .create_function(move |_, key: String| {
            let mut context = context.borrow_mut();
            context.memory_delete(&key).map_err(mlua::Error::external)
        })
        ?;
    table.set("delete", delete)?;
    Ok(table)
}

fn create_message_table(
    lua: &Lua,
    message: &Message,
    access_plan: &MessageAccessPlan,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    if let Some(source) = access_plan.source.as_ref() {
        if access_enabled(source.mode) {
            table.set("source", create_source_table_with_access(lua, message, source)?)?;
        }
    }
    if let Some(topic) = access_plan.topic.as_ref() {
        if access_enabled(topic.mode) {
            table.set("topic", create_topic_table_with_access(lua, message, topic)?)?;
        }
    }
    if let Some(payload) = access_plan.payload.as_ref() {
        if access_enabled(payload.mode) {
            table.set("payload", create_payload_value_with_access(lua, &message.payload, payload)?)?;
        }
    }
    if let Some(raw_payload) = access_plan.raw_payload.as_ref() {
        if access_enabled(raw_payload.mode) {
            table.set("raw_payload", create_raw_payload_table_with_access(lua, message, raw_payload)?)?;
        }
    }
    if let Some(metadata) = access_plan.metadata.as_ref() {
        if access_enabled(metadata.mode) {
            table.set("metadata", create_metadata_table_with_access(lua, message, metadata)?)?;
        }
    }
    if let Some(properties) = access_plan.properties.as_ref() {
        if access_enabled(properties.mode) {
            table.set("properties", create_properties_table_with_access(lua, message, properties)?)?;
        }
    }
    Ok(table)
}

fn access_enabled(mode: i32) -> bool {
    mode != AccessMode::None as i32
}

fn full_message_access_plan() -> MessageAccessPlan {
    MessageAccessPlan {
        source: Some(SourceAccess {
            mode: AccessMode::Full as i32,
            name: false,
            version: false,
        }),
        topic: Some(TopicAccess {
            mode: AccessMode::Full as i32,
            raw: false,
            levels: false,
            level_indexes: Vec::new(),
        }),
        payload: Some(JsonAccess {
            mode: AccessMode::Full as i32,
            paths: Vec::new(),
        }),
        raw_payload: Some(RawPayloadAccess {
            mode: AccessMode::Full as i32,
            ranges: Vec::new(),
        }),
        metadata: Some(MetadataAccess {
            mode: AccessMode::Full as i32,
            pkid: false,
            qos: false,
            retain: false,
            dup: false,
        }),
        properties: Some(PropertiesAccess {
            mode: AccessMode::Full as i32,
            keys: Vec::new(),
        }),
    }
}

fn create_source_table(
    lua: &Lua,
    source: &tenon_extension::SourceContext,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("name", source.name.as_str())?;
    table.set("version", source.version.as_str())?;
    Ok(table)
}

fn create_source_table_with_access(
    lua: &Lua,
    message: &Message,
    access: &SourceAccess,
) -> mlua::Result<Table> {
    if access.mode == AccessMode::Full as i32 {
        return create_source_table(lua, &message.source);
    }
    let table = lua.create_table()?;
    if access.name {
        table.set("name", message.source.name.as_str())?;
    }
    if access.version {
        table.set("version", message.source.version.as_str())?;
    }
    Ok(table)
}

fn create_topic_table(lua: &Lua, message: &Message) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("raw", message.topic.raw.as_str())?;
    let levels = lua.create_table()?;
    for (index, level) in message.topic.levels.iter().enumerate() {
        levels.set(index + 1, level.as_str())?;
    }
    table.set("levels", levels)?;
    Ok(table)
}

fn create_topic_table_with_access(
    lua: &Lua,
    message: &Message,
    access: &TopicAccess,
) -> mlua::Result<Table> {
    if access.mode == AccessMode::Full as i32 {
        return create_topic_table(lua, message);
    }
    let table = lua.create_table()?;
    if access.raw {
        table.set("raw", message.topic.raw.as_str())?;
    }
    if access.levels || !access.level_indexes.is_empty() {
        let levels = lua.create_table()?;
        if access.levels {
            for (index, level) in message.topic.levels.iter().enumerate() {
                levels.set(index + 1, level.as_str())?;
            }
        } else {
            for lua_index in &access.level_indexes {
                if let Some(level) = message.topic.lua_level(*lua_index as usize) {
                    levels.set(*lua_index, level)?;
                }
            }
        }
        table.set("levels", levels)?;
    }
    Ok(table)
}

fn create_raw_payload_table(lua: &Lua, message: &Message) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, byte) in message.raw_payload.iter().enumerate() {
        table.set(index + 1, *byte)?;
    }
    Ok(table)
}

fn create_raw_payload_table_with_access(
    lua: &Lua,
    message: &Message,
    access: &RawPayloadAccess,
) -> mlua::Result<Table> {
    if access.mode == AccessMode::Full as i32 {
        return create_raw_payload_table(lua, message);
    }
    let table = lua.create_table()?;
    for range in &access.ranges {
        let start = range.offset as usize;
        let end = start.saturating_add(range.length as usize).min(message.raw_payload.len());
        for offset in start..end {
            table.set(offset + 1, message.raw_payload[offset])?;
        }
    }
    Ok(table)
}

fn create_metadata_table(lua: &Lua, message: &Message) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("pkid", message.metadata.pkid)?;
    table.set("qos", message.metadata.qos)?;
    table.set("retain", message.metadata.retain)?;
    table.set("dup", message.metadata.dup)?;
    Ok(table)
}

fn create_metadata_table_with_access(
    lua: &Lua,
    message: &Message,
    access: &MetadataAccess,
) -> mlua::Result<Table> {
    if access.mode == AccessMode::Full as i32 {
        return create_metadata_table(lua, message);
    }
    let table = lua.create_table()?;
    if access.pkid {
        table.set("pkid", message.metadata.pkid)?;
    }
    if access.qos {
        table.set("qos", message.metadata.qos)?;
    }
    if access.retain {
        table.set("retain", message.metadata.retain)?;
    }
    if access.dup {
        table.set("dup", message.metadata.dup)?;
    }
    Ok(table)
}

fn create_properties_table(lua: &Lua, message: &Message) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (key, value) in &message.properties {
        table.set(key.as_str(), value.as_str())?;
    }
    Ok(table)
}

fn create_properties_table_with_access(
    lua: &Lua,
    message: &Message,
    access: &PropertiesAccess,
) -> mlua::Result<Table> {
    if access.mode == AccessMode::Full as i32 {
        return create_properties_table(lua, message);
    }
    let table = lua.create_table()?;
    for key in &access.keys {
        if let Some(value) = message.properties.get(key) {
            table.set(key.as_str(), value.as_str())?;
        }
    }
    Ok(table)
}

fn create_payload_value_with_access(
    lua: &Lua,
    payload: &ExtensionValue,
    access: &JsonAccess,
) -> mlua::Result<Value> {
    if access.mode == AccessMode::Full as i32 {
        return json_to_lua_value(lua, payload);
    }
    let table = lua.create_table()?;
    for path in &access.paths {
        materialize_json_path(lua, &table, payload, &path.segments)?;
    }
    Ok(Value::Table(table))
}

fn materialize_json_path(
    lua: &Lua,
    table: &Table,
    payload: &ExtensionValue,
    path: &[JsonPathSegment],
) -> mlua::Result<()> {
    let Some(value) = json_value_at_path(payload, path) else {
        return Ok(());
    };
    set_lua_path(lua, table, path, value)
}

fn json_value_at_path<'a>(
    value: &'a ExtensionValue,
    path: &[JsonPathSegment],
) -> Option<&'a ExtensionValue> {
    let mut current = value;
    for segment in path {
        match segment.kind.as_ref()? {
            json_path_segment::Kind::Field(field) => {
                current = current.as_object()?.get(field)?;
            }
            json_path_segment::Kind::Index(index) => {
                let zero_based = index.checked_sub(1)? as usize;
                current = current.as_array()?.get(zero_based)?;
            }
        }
    }
    Some(current)
}

fn set_lua_path(
    lua: &Lua,
    table: &Table,
    path: &[JsonPathSegment],
    value: &ExtensionValue,
) -> mlua::Result<()> {
    let Some((last, parents)) = path.split_last() else {
        return Ok(());
    };
    let mut current = table.clone();
    for segment in parents {
        let next = match segment.kind.as_ref() {
            Some(json_path_segment::Kind::Field(field)) => match current.get::<Value>(field.as_str())? {
                Value::Table(table) => table,
                _ => {
                    let table = lua.create_table()?;
                    current.set(field.as_str(), table.clone())?;
                    table
                }
            },
            Some(json_path_segment::Kind::Index(index)) => match current.get::<Value>(*index)? {
                Value::Table(table) => table,
                _ => {
                    let table = lua.create_table()?;
                    current.set(*index, table.clone())?;
                    table
                }
            },
            None => return Err(mlua::Error::external("JSON path segment kind is missing")),
        };
        current = next;
    }
    match last.kind.as_ref() {
        Some(json_path_segment::Kind::Field(field)) => current.set(field.as_str(), json_to_lua_value(lua, value)?),
        Some(json_path_segment::Kind::Index(index)) => current.set(*index, json_to_lua_value(lua, value)?),
        None => Err(mlua::Error::external("JSON path segment kind is missing")),
    }
}

fn json_to_lua_value(lua: &Lua, value: &ExtensionValue) -> mlua::Result<Value> {
    match value {
        ExtensionValue::Null => Ok(Value::Nil),
        ExtensionValue::Bool(value) => Ok(Value::Boolean(*value)),
        ExtensionValue::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Value::Integer(value))
            } else if let Some(value) = value.as_u64().and_then(|value| i64::try_from(value).ok()) {
                Ok(Value::Integer(value))
            } else if let Some(value) = value.as_f64() {
                Ok(Value::Number(value))
            } else {
                Ok(Value::Nil)
            }
        }
        ExtensionValue::String(value) => Ok(Value::String(lua.create_string(value)?)),
        ExtensionValue::Array(values) => {
            let table = lua.create_table()?;
            for (index, value) in values.iter().enumerate() {
                table.set(index + 1, json_to_lua_value(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
        ExtensionValue::Object(values) => {
            let table = lua.create_table()?;
            for (key, value) in values {
                table.set(key.as_str(), json_to_lua_value(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

fn lua_value_to_json(value: Value) -> mlua::Result<ExtensionValue> {
    match value {
        Value::Nil => Ok(ExtensionValue::Null),
        Value::Boolean(value) => Ok(ExtensionValue::Bool(value)),
        Value::Integer(value) => Ok(ExtensionValue::Number(Number::from(value))),
        Value::Number(value) => Number::from_f64(value)
            .map(ExtensionValue::Number)
            .ok_or_else(|| mlua::Error::external("non-finite Lua number cannot be emitted")),
        Value::String(value) => Ok(ExtensionValue::String(value.to_string_lossy())),
        Value::Table(table) => lua_table_to_json(table),
        other => Err(mlua::Error::external(format!(
            "unsupported Lua value for JSON conversion: {}",
            other.type_name()
        ))),
    }
}

fn lua_value_to_emit_payload(value: Value) -> mlua::Result<ExtensionValue> {
    let payload = lua_value_to_json(value)?;
    if payload.is_object() {
        Ok(payload)
    } else {
        Err(mlua::Error::external("ctx.emit payload must be a JSON object"))
    }
}

fn validate_on_message_return(value: Value) -> mlua::Result<()> {
    match value {
        Value::Nil => Ok(()),
        _ => Err(mlua::Error::external("on_message must not return a value")),
    }
}

fn lua_table_to_json(table: Table) -> mlua::Result<ExtensionValue> {
    let mut entries = Vec::new();
    let mut array_len = 0usize;
    let mut array_shaped = true;
    for entry in table.pairs::<Value, Value>() {
        let (key, value) = entry?;
        if let Value::Integer(index) = key {
            if index > 0 {
                array_len = array_len.max(index as usize);
            } else {
                array_shaped = false;
            }
            entries.push((Value::Integer(index), value));
        } else {
            array_shaped = false;
            entries.push((key, value));
        }
    }
    if array_shaped {
        let mut values = vec![ExtensionValue::Null; array_len];
        for (key, value) in entries {
            if let Value::Integer(index) = key {
                values[index as usize - 1] = lua_value_to_json(value)?;
            }
        }
        return Ok(ExtensionValue::Array(values));
    }
    let mut values = Map::new();
    for (key, value) in entries {
        let key = lua_key_to_json_key(key)?;
        values.insert(key, lua_value_to_json(value)?);
    }
    Ok(ExtensionValue::Object(values))
}

fn lua_key_to_json_key(key: Value) -> mlua::Result<String> {
    match key {
        Value::String(value) => Ok(value.to_string_lossy()),
        Value::Integer(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Boolean(value) => Ok(value.to_string()),
        other => Err(mlua::Error::external(format!(
            "unsupported Lua table key for JSON conversion: {}",
            other.type_name()
        ))),
    }
}

const DEFAULT_LUA_INSTRUCTION_BUDGET: u64 = 1_000_000;
const LUA_HOOK_INSTRUCTION_INTERVAL: u32 = 1_000;

fn safe_lua_libs() -> StdLib {
    StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH
}

fn install_instruction_budget_hook(lua: &Lua, instruction_budget: Arc<AtomicU64>) {
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(LUA_HOOK_INSTRUCTION_INTERVAL),
        move |_, _| {
            let remaining = instruction_budget.fetch_update(
                Ordering::Relaxed,
                Ordering::Relaxed,
                |remaining| remaining.checked_sub(LUA_HOOK_INSTRUCTION_INTERVAL as u64),
            );
            match remaining {
                Ok(_) => Ok(VmState::Continue),
                Err(_) => Err(mlua::Error::external("Lua instruction budget exceeded")),
            }
        },
    );
}

fn lua_error(error: mlua::Error) -> WorkerError {
    WorkerError::processor(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use tenon_extension::{MqttMetadata, SourceContext, Topic};
    use tenon_message::daemon::v1::json_path_segment;

    #[test]
    fn owns_context() {
        let processor = Box::new(LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: "function on_message(ctx, msg) end".to_string(),
            access_plan: None,
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor"));

        let context = processor.into_context();

        assert_eq!(context.source().name, "p");
    }

    #[test]
    fn emits_payload() {
        let mut processor = LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: "function on_message(ctx, msg) ctx.emit(msg.payload) end".to_string(),
            access_plan: None,
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor");

        let outcome = processor.process(&message(json!({"temp": 30}))).expect("outcome");

        assert_eq!(outcome.emits.len(), 1);
        assert_eq!(outcome.emits[0].payload, json!({"temp": 30}));
    }

    #[test]
    fn materializes_only_planned_message_fields() {
        let mut processor = LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: r#"
                function on_message(ctx, msg)
                  ctx.emit({
                    source_name = msg.source.name,
                    source_version_missing = msg.source.version == nil,
                    topic_raw = msg.topic.raw,
                    topic_level2 = msg.topic.levels[2],
                    topic_level1_missing = msg.topic.levels[1] == nil,
                    temp = msg.payload.temp,
                    hum_missing = msg.payload.hum == nil,
                    first_byte = msg.raw_payload[1],
                    second_byte_missing = msg.raw_payload[2] == nil,
                    qos = msg.metadata.qos,
                    pkid_missing = msg.metadata.pkid == nil,
                    site = msg.properties.site,
                    other_missing = msg.properties.other == nil
                  })
                end
            "#.to_string(),
            access_plan: Some(selective_access_plan()),
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor");

        let outcome = processor.process(&message_with(
            "sensor/room1",
            json!({"temp": 30, "hum": 10}),
            MqttMetadata::new(7, 1, false, false),
            [("site", "lab"), ("other", "x")],
        )).expect("outcome");

        assert_eq!(outcome.emits[0].payload, json!({
            "source_name": "source",
            "source_version_missing": true,
            "topic_raw": "sensor/room1",
            "topic_level2": "room1",
            "topic_level1_missing": true,
            "temp": 30,
            "hum_missing": true,
            "first_byte": 123,
            "second_byte_missing": true,
            "qos": 1,
            "pkid_missing": true,
            "site": "lab",
            "other_missing": true
        }));
    }

    #[test]
    fn keeps_memory_between_messages() {
        let mut processor = LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: r#"
                function on_message(ctx, msg)
                  local count = ctx.memory.get("count")
                  if count == nil then count = 0 end
                  count = count + 1
                  ctx.memory.set("count", count)
                  ctx.emit({count = count})
                end
            "#.to_string(),
            access_plan: None,
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor");

        let first = processor.process(&message(json!({"temp": 30}))).expect("first");
        let second = processor.process(&message(json!({"temp": 31}))).expect("second");

        assert_eq!(first.emits[0].payload, json!({"count": 1}));
        assert_eq!(second.emits[0].payload, json!({"count": 2}));
    }

    #[test]
    fn can_predicate_on_mqtt_fields_and_payload() {
        let mut processor = LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: r#"
                function on_message(ctx, msg)
                  if msg.topic.levels[1] == "sensor"
                    and msg.topic.levels[2] == "room1"
                    and msg.metadata.qos == 1
                    and msg.metadata.retain == false
                    and msg.properties.site == "lab"
                    and msg.payload.temp > 30
                  then
                    ctx.emit({
                      topic = msg.topic.raw,
                      site = msg.properties.site,
                      temp = msg.payload.temp,
                      pkid = msg.metadata.pkid
                    })
                  end
                end
            "#.to_string(),
            access_plan: None,
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor");

        let skipped = processor.process(&message_with(
            "sensor/room1",
            json!({"temp": 20}),
            MqttMetadata::new(7, 1, false, false),
            [("site", "lab")],
        )).expect("skipped");
        let emitted = processor.process(&message_with(
            "sensor/room1",
            json!({"temp": 31}),
            MqttMetadata::new(8, 1, false, false),
            [("site", "lab")],
        )).expect("emitted");

        assert!(skipped.emits.is_empty());
        assert_eq!(emitted.emits.len(), 1);
        assert_eq!(emitted.emits[0].payload, json!({
            "topic": "sensor/room1",
            "site": "lab",
            "temp": 31,
            "pkid": 8
        }));
    }

    #[test]
    fn can_aggregate_context_and_emit_alert() {
        let mut processor = LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: r#"
                function on_message(ctx, msg)
                  local hot_count = ctx.memory.get("hot_count")
                  if hot_count == nil then hot_count = 0 end

                  if msg.payload.temp > 30 then
                    hot_count = hot_count + 1
                    ctx.memory.set("hot_count", hot_count)
                  end

                  if hot_count > 5 then
                    ctx.emit({
                      kind = "temp_alert",
                      count = hot_count,
                      temp = msg.payload.temp
                    })
                  end
                end
            "#.to_string(),
            access_plan: None,
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor");

        let cold = processor.process(&message(json!({"temp": 10}))).expect("cold");
        assert!(cold.emits.is_empty());

        for _ in 0..5 {
            let outcome = processor.process(&message(json!({"temp": 31}))).expect("hot");
            assert!(outcome.emits.is_empty());
        }

        let alert = processor.process(&message(json!({"temp": 32}))).expect("alert");

        assert_eq!(alert.emits.len(), 1);
        assert_eq!(alert.emits[0].payload, json!({
            "kind": "temp_alert",
            "count": 6,
            "temp": 32
        }));
    }

    #[test]
    fn blocks_unsafe_standard_libraries() {
        let mut processor = LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: r#"
                function on_message(ctx, msg)
                  ctx.emit({has_os = os ~= nil, has_io = io ~= nil, has_package = package ~= nil})
                end
            "#.to_string(),
            access_plan: None,
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor");

        let outcome = processor.process(&message(json!({"temp": 30}))).expect("outcome");

        assert_eq!(outcome.emits[0].payload, json!({
            "has_os": false,
            "has_io": false,
            "has_package": false
        }));
    }

    #[test]
    fn rejects_infinite_loop() {
        let mut processor = LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: r#"
                function on_message(ctx, msg)
                  while true do end
                end
            "#.to_string(),
            access_plan: None,
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor");

        let error = processor
            .process(&message(json!({"temp": 30})))
            .expect_err("instruction budget error");

        assert_eq!(error.kind, crate::WorkerErrorKind::Processor);
        assert!(error.message.contains("instruction budget"));
    }

    #[test]
    fn rejects_on_message_return_value() {
        let mut processor = LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: r#"
                function on_message(ctx, msg)
                  return { temp = msg.payload.temp }
                end
            "#.to_string(),
            access_plan: None,
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor");

        let error = processor
            .process(&message(json!({"temp": 30})))
            .expect_err("return value error");

        assert_eq!(error.kind, crate::WorkerErrorKind::Processor);
        assert!(error.message.contains("on_message must not return"));
    }

    #[test]
    fn rejects_non_object_emit_payload() {
        let mut processor = LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: r#"
                function on_message(ctx, msg)
                  ctx.emit(msg.payload)
                end
            "#.to_string(),
            access_plan: None,
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor");

        let error = processor
            .process(&message(json!([1, 2, 3])))
            .expect_err("non-object emit payload");

        assert_eq!(error.kind, crate::WorkerErrorKind::Processor);
        assert!(error.message.contains("ctx.emit payload must be a JSON object"));
    }

    fn message(payload: ExtensionValue) -> Message {
        message_with(
            "sensor/a",
            payload,
            MqttMetadata::new(1, 1, false, false),
            [],
        )
    }

    fn message_with<const N: usize>(
        topic: &str,
        payload: ExtensionValue,
        metadata: MqttMetadata,
        properties: [(&str, &str); N],
    ) -> Message {
        Message::new(
            SourceContext::new("source", "r1"),
            Topic::new(topic),
            payload,
            br#"{"temp":30}"#.to_vec(),
            metadata,
            properties
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<HashMap<_, _>>(),
        )
    }

    fn selective_access_plan() -> MessageAccessPlan {
        MessageAccessPlan {
            source: Some(SourceAccess {
                mode: AccessMode::Selective as i32,
                name: true,
                version: false,
            }),
            topic: Some(TopicAccess {
                mode: AccessMode::Selective as i32,
                raw: true,
                levels: false,
                level_indexes: vec![2],
            }),
            payload: Some(JsonAccess {
                mode: AccessMode::Selective as i32,
                paths: vec![json_path_field("temp")],
            }),
            raw_payload: Some(RawPayloadAccess {
                mode: AccessMode::Selective as i32,
                ranges: vec![tenon_message::plan::ByteRange {
                    offset: 0,
                    length: 1,
                }],
            }),
            metadata: Some(MetadataAccess {
                mode: AccessMode::Selective as i32,
                pkid: false,
                qos: true,
                retain: false,
                dup: false,
            }),
            properties: Some(PropertiesAccess {
                mode: AccessMode::Selective as i32,
                keys: vec!["site".to_string()],
            }),
        }
    }

    fn json_path_field(field: &str) -> tenon_message::plan::JsonPath {
        tenon_message::plan::JsonPath {
            segments: vec![tenon_message::plan::JsonPathSegment {
                kind: Some(json_path_segment::Kind::Field(field.to_string())),
            }],
        }
    }
}
