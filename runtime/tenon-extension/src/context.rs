use crate::{
    EmitRecord, ExtensionError, ExtensionResult, ExtensionValue, InvocationOutcome, SourceContext,
    ScriptApi,
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Context {
    source: SourceContext,
    memory: MemoryView,
    emitter: EmitBuffer,
}

impl Context {
    pub fn new(source: SourceContext, memory: MemoryView, emitter: EmitBuffer) -> Self {
        Self {
            source,
            memory,
            emitter,
        }
    }

    pub fn with_empty_memory(source: SourceContext) -> Self {
        Self::new(source, MemoryView::default(), EmitBuffer::default())
    }

    pub fn source(&self) -> &SourceContext {
        &self.source
    }

    pub fn memory(&self) -> &MemoryView {
        &self.memory
    }

    pub fn memory_mut(&mut self) -> &mut MemoryView {
        &mut self.memory
    }

    pub fn emitter(&self) -> &EmitBuffer {
        &self.emitter
    }

    pub fn emitter_mut(&mut self) -> &mut EmitBuffer {
        &mut self.emitter
    }

    pub fn memory_get(&self, key: &str) -> ExtensionResult<Option<ExtensionValue>> {
        self.memory.get(key)
    }

    pub fn memory_set(
        &mut self,
        key: impl Into<String>,
        value: ExtensionValue,
    ) -> ExtensionResult<()> {
        self.memory.set(key.into(), value)
    }

    pub fn memory_delete(&mut self, key: &str) -> ExtensionResult<()> {
        self.memory.delete(key)
    }

    pub fn emit(&mut self, payload: ExtensionValue) -> ExtensionResult<()> {
        self.emitter.emit(EmitRecord::new(payload))
    }

    pub fn drain_outcome(&mut self) -> InvocationOutcome {
        InvocationOutcome {
            emits: std::mem::take(&mut self.emitter).into_records(),
        }
    }
}

impl ScriptApi for Context {
    const FIELDS: &'static [&'static str] = &["memory", "source", "emit"];
}

impl Context {
    pub fn into_parts(self) -> (SourceContext, MemoryView, EmitBuffer) {
        (self.source, self.memory, self.emitter)
    }

    pub fn into_outcome(self) -> InvocationOutcome {
        InvocationOutcome {
            emits: self.emitter.into_records(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemoryView {
    values: HashMap<String, ExtensionValue>,
}

impl MemoryView {
    pub fn values(&self) -> &HashMap<String, ExtensionValue> {
        &self.values
    }

    pub fn get(&self, key: &str) -> ExtensionResult<Option<ExtensionValue>> {
        Ok(self.values.get(key).cloned())
    }

    pub fn set(&mut self, key: impl Into<String>, value: ExtensionValue) -> ExtensionResult<()> {
        let key = key.into();
        if key.is_empty() {
            return Err(ExtensionError::invalid_argument("memory key is empty"));
        }
        self.values.insert(key, value);
        Ok(())
    }

    pub fn delete(&mut self, key: &str) -> ExtensionResult<()> {
        if key.is_empty() {
            return Err(ExtensionError::invalid_argument("memory key is empty"));
        }
        self.values.remove(key);
        Ok(())
    }
}

impl ScriptApi for MemoryView {
    const METHODS: &'static [&'static str] = &["get", "set", "delete"];
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmitBuffer {
    records: Vec<EmitRecord>,
}

impl EmitBuffer {
    pub fn records(&self) -> &[EmitRecord] {
        &self.records
    }

    pub fn into_records(self) -> Vec<EmitRecord> {
        self.records
    }
}

impl EmitBuffer {
    pub fn emit(&mut self, record: EmitRecord) -> ExtensionResult<()> {
        self.records.push(record);
        Ok(())
    }
}
