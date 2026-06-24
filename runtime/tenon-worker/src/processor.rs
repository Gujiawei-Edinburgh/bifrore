use crate::{WorkerError, WorkerResult};
use mlua::{Function, Lua, Scope, Table, Value};
use serde_json::{Map, Number};
use std::cell::RefCell;
use tenon_extension::{
    Context, ExtensionValue, InvocationOutcome, Message, PROCESS_ON_MESSAGE_FN,
};
use tenon_message::plan::ProcessPlan;

pub trait Processor: Send + 'static {
    fn process(&mut self, message: &Message) -> WorkerResult<InvocationOutcome>;

    fn into_context(self: Box<Self>) -> Context;
}

#[derive(Debug)]
pub struct LuaProcessor {
    lua: Lua,
    context: Context,
}

impl LuaProcessor {
    pub fn new(process: ProcessPlan, context: Context) -> WorkerResult<Self> {
        let lua = Lua::new();
        lua.load(&process.source)
            .exec()
            .map_err(lua_error)?;
        let _: Function = lua
            .globals()
            .get(PROCESS_ON_MESSAGE_FN)
            .map_err(lua_error)?;
        Ok(Self { lua, context })
    }
}

impl Processor for LuaProcessor {
    fn process(&mut self, message: &Message) -> WorkerResult<InvocationOutcome> {
        let function: Function = self
            .lua
            .globals()
            .get(PROCESS_ON_MESSAGE_FN)
            .map_err(lua_error)?;
        let context = RefCell::new(&mut self.context);
        self.lua
            .scope(|scope| {
                let ctx = create_context_table(&self.lua, scope, &context)?;
                let msg = create_message_table(&self.lua, message)?;
                function.call::<()>((ctx, msg))
            })
            .map_err(lua_error)?;
        Ok(self.context.drain_outcome())
    }

    fn into_context(self: Box<Self>) -> Context {
        let Self {
            lua,
            context,
        } = *self;
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
            let payload = lua_value_to_json(payload)?;
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

fn create_message_table(lua: &Lua, message: &Message) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("source", create_source_table(lua, &message.source)?)?;
    table.set("topic", create_topic_table(lua, message)?)?;
    table.set("payload", json_to_lua_value(lua, &message.payload)?)?;
    table.set("raw_payload", create_raw_payload_table(lua, message)?)?;
    table.set("metadata", create_metadata_table(lua, message)?)?;
    table.set("properties", create_properties_table(lua, message)?)?;
    Ok(table)
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

fn create_raw_payload_table(lua: &Lua, message: &Message) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, byte) in message.raw_payload.iter().enumerate() {
        table.set(index + 1, *byte)?;
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

fn create_properties_table(lua: &Lua, message: &Message) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (key, value) in &message.properties {
        table.set(key.as_str(), value.as_str())?;
    }
    Ok(table)
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

fn lua_error(error: mlua::Error) -> WorkerError {
    WorkerError::processor(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use tenon_extension::{MqttMetadata, SourceContext, Topic};

    #[test]
    fn owns_context() {
        let processor = Box::new(LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: "function on_message(ctx, msg) end".to_string(),
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor"));

        let context = processor.into_context();

        assert_eq!(context.source().name, "p");
    }

    #[test]
    fn emits_payload() {
        let mut processor = LuaProcessor::new(ProcessPlan {
            runtime: tenon_message::plan::ScriptRuntime::Lua as i32,
            source: "function on_message(ctx, msg) ctx.emit(msg.payload) end".to_string(),
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor");

        let outcome = processor.process(&message(json!({"temp": 30}))).expect("outcome");

        assert_eq!(outcome.emits.len(), 1);
        assert_eq!(outcome.emits[0].payload, json!({"temp": 30}));
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
        }, Context::with_empty_memory(SourceContext::new("p", "r1"))).expect("processor");

        let first = processor.process(&message(json!({"temp": 30}))).expect("first");
        let second = processor.process(&message(json!({"temp": 31}))).expect("second");

        assert_eq!(first.emits[0].payload, json!({"count": 1}));
        assert_eq!(second.emits[0].payload, json!({"count": 2}));
    }

    fn message(payload: ExtensionValue) -> Message {
        Message::new(
            SourceContext::new("source", "r1"),
            Topic::new("sensor/a"),
            payload,
            br#"{"temp":30}"#.to_vec(),
            MqttMetadata::new(1, 1, false, false),
            HashMap::new(),
        )
    }
}
