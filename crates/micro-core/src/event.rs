use std::collections::VecDeque;

use micro_ir::FunctionId;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Activate(FunctionId),
    /// A `ui.input` field changed; the handler receives the new text as its
    /// single runtime argument.
    InputChanged(FunctionId, String),
    /// A `ui.slider` position changed; the handler receives the new value as
    /// its single runtime argument.
    SliderChanged(FunctionId, f64),
    /// A `ui.checkbox` toggled; the handler receives the new checked state.
    CheckedChanged(FunctionId, bool),
    /// A selection widget (ui.dropdown / ui.roller) changed; the handler
    /// receives the newly selected option index.
    SelectionChanged(FunctionId, f64),
}

#[derive(Debug, Default)]
pub struct EventQueue {
    events: VecDeque<Event>,
}

impl EventQueue {
    pub fn push(&mut self, event: Event) {
        self.events.push_back(event);
    }

    pub fn pop(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}
