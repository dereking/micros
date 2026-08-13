use std::collections::BTreeSet;

use micro_ir::{AppImage, StateId};
use micro_vm::{StateAccess, StateError, Value};

pub struct StateStore {
    slots: Vec<Value>,
    reads: Option<BTreeSet<StateId>>,
    changed: BTreeSet<StateId>,
}

impl StateStore {
    pub fn from_image(image: &AppImage) -> Self {
        let slots = image
            .states
            .iter()
            .map(|state| Value::from(&image.constants[state.initial as usize]))
            .collect();
        Self {
            slots,
            reads: None,
            changed: BTreeSet::new(),
        }
    }

    pub fn get(&self, id: StateId) -> Option<&Value> {
        self.slots.get(id.0 as usize)
    }

    pub(crate) fn begin_tracking(&mut self) {
        self.reads = Some(BTreeSet::new());
    }

    pub(crate) fn finish_tracking(&mut self) -> BTreeSet<StateId> {
        self.reads.take().unwrap_or_default()
    }

    pub(crate) fn take_changed(&mut self) -> BTreeSet<StateId> {
        std::mem::take(&mut self.changed)
    }
}

impl StateAccess for StateStore {
    fn read(&mut self, id: StateId) -> Result<Value, StateError> {
        let value = self
            .slots
            .get(id.0 as usize)
            .cloned()
            .ok_or(StateError::OutOfRange(id))?;
        if let Some(reads) = &mut self.reads {
            reads.insert(id);
        }
        Ok(value)
    }

    fn write(&mut self, id: StateId, value: Value) -> Result<(), StateError> {
        let slot = self
            .slots
            .get_mut(id.0 as usize)
            .ok_or(StateError::OutOfRange(id))?;
        if slot.scalar_type() != value.scalar_type() {
            return Err(StateError::TypeMismatch {
                expected: slot.type_name(),
                found: value.type_name(),
            });
        }
        if *slot != value {
            *slot = value;
            self.changed.insert(id);
        }
        Ok(())
    }
}
