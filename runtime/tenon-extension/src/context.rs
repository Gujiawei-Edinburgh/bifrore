use crate::{
    EmitRecord, ExtensionError, ExtensionResult, ExtensionValue, InvocationOutcome, SourceContext,
    StateMutation, ScriptApi,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug)]
pub struct Context {
    source: SourceContext,
    state: StateView,
    emitter: EmitBuffer,
}

impl Context {
    pub fn new(source: SourceContext, state: StateView, emitter: EmitBuffer) -> Self {
        Self {
            source,
            state,
            emitter,
        }
    }

    pub fn from_snapshot(source: SourceContext, snapshot: StateSnapshot) -> Self {
        Self::new(source, StateView::from_snapshot(snapshot), EmitBuffer::default())
    }

    pub fn source(&self) -> &SourceContext {
        &self.source
    }

    pub fn state(&self) -> &StateView {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut StateView {
        &mut self.state
    }

    pub fn emitter(&self) -> &EmitBuffer {
        &self.emitter
    }

    pub fn emitter_mut(&mut self) -> &mut EmitBuffer {
        &mut self.emitter
    }

    pub fn state_get(&self, key: &str) -> ExtensionResult<Option<ExtensionValue>> {
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

    pub fn emit(&mut self, payload: ExtensionValue) -> ExtensionResult<()> {
        self.emitter.emit(EmitRecord::new(payload))
    }
}

impl ScriptApi for Context {
    const FIELDS: &'static [&'static str] = &["state", "source", "emit"];
}

impl Context {
    pub fn into_parts(self) -> (SourceContext, StateView, EmitBuffer) {
        (self.source, self.state, self.emitter)
    }

    pub fn into_outcome(self) -> InvocationOutcome {
        InvocationOutcome {
            state_delta: self.state.into_delta(),
            emits: self.emitter.into_records(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StateSnapshot {
    values: HashMap<String, ExtensionValue>,
}

impl StateSnapshot {
    pub fn new(values: HashMap<String, ExtensionValue>) -> Self {
        Self { values }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn values(&self) -> &HashMap<String, ExtensionValue> {
        &self.values
    }

    pub fn get(&self, key: &str) -> Option<&ExtensionValue> {
        self.values.get(key)
    }

    pub fn into_inner(self) -> HashMap<String, ExtensionValue> {
        self.values
    }

    pub fn into_view(self) -> StateView {
        StateView::from_snapshot(self)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StateView {
    values: HashMap<String, ExtensionValue>,
    delta: Vec<StateMutation>,
}

impl StateView {
    pub fn from_snapshot(snapshot: StateSnapshot) -> Self {
        Self {
            values: snapshot.into_inner(),
            delta: Vec::new(),
        }
    }

    pub fn values(&self) -> &HashMap<String, ExtensionValue> {
        &self.values
    }

    pub fn delta(&self) -> &[StateMutation] {
        &self.delta
    }

    pub fn into_delta(self) -> Vec<StateMutation> {
        self.delta
    }

    pub fn get(&self, key: &str) -> ExtensionResult<Option<ExtensionValue>> {
        Ok(self.values.get(key).cloned())
    }

    pub fn set(&mut self, key: impl Into<String>, value: ExtensionValue) -> ExtensionResult<()> {
        let key = key.into();
        if key.is_empty() {
            return Err(ExtensionError::invalid_argument("state key is empty"));
        }
        self.values.insert(key.clone(), value.clone());
        self.delta.push(StateMutation::Set { key, value });
        Ok(())
    }

    pub fn delete(&mut self, key: &str) -> ExtensionResult<()> {
        if key.is_empty() {
            return Err(ExtensionError::invalid_argument("state key is empty"));
        }
        self.values.remove(key);
        self.delta.push(StateMutation::Delete {
            key: key.to_string(),
        });
        Ok(())
    }
}

impl ScriptApi for StateView {
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
