use crate::{EmitRecord, ExtensionError, ExtensionResult, ExtensionValue, SourceContext};
use std::collections::HashMap;

#[derive(Debug)]
pub struct Context {
    source: SourceContext,
    state: State,
    emitter: EmitBuffer,
}

impl Context {
    pub fn new(source: SourceContext, state: State, emitter: EmitBuffer) -> Self {
        Self {
            source,
            state,
            emitter,
        }
    }

    pub fn source(&self) -> &SourceContext {
        &self.source
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }

    pub fn emitter(&self) -> &EmitBuffer {
        &self.emitter
    }

    pub fn emitter_mut(&mut self) -> &mut EmitBuffer {
        &mut self.emitter
    }

    pub fn state_get(&mut self, key: &str) -> ExtensionResult<Option<ExtensionValue>> {
        self.state.get(key)
    }

    pub fn state_set(
        &mut self,
        key: impl Into<String>,
        value: ExtensionValue,
    ) -> ExtensionResult<()> {
        self.state.set(key.into(), value)
    }

    pub fn state_delete(&mut self, key: &str) -> ExtensionResult<()> {
        self.state.delete(key)
    }

    pub fn emit(
        &mut self,
        channel: impl Into<String>,
        payload: ExtensionValue,
    ) -> ExtensionResult<()> {
        self.emitter.emit(EmitRecord::new(channel, payload))
    }
}

impl Context {
    pub fn into_parts(self) -> (SourceContext, State, EmitBuffer) {
        (self.source, self.state, self.emitter)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    values: HashMap<String, ExtensionValue>,
}

impl State {
    pub fn new(values: HashMap<String, ExtensionValue>) -> Self {
        Self { values }
    }

    pub fn into_inner(self) -> HashMap<String, ExtensionValue> {
        self.values
    }

    pub fn get(&mut self, key: &str) -> ExtensionResult<Option<ExtensionValue>> {
        Ok(self.values.get(key).cloned())
    }

    pub fn set(&mut self, key: impl Into<String>, value: ExtensionValue) -> ExtensionResult<()> {
        let key = key.into();
        if key.is_empty() {
            return Err(ExtensionError::invalid_argument("state key is empty"));
        }
        self.values.insert(key, value);
        Ok(())
    }

    pub fn delete(&mut self, key: &str) -> ExtensionResult<()> {
        self.values.remove(key);
        Ok(())
    }
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
        if record.channel.is_empty() {
            return Err(ExtensionError::invalid_argument("emit channel is empty"));
        }
        self.records.push(record);
        Ok(())
    }
}
