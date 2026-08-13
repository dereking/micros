use std::collections::VecDeque;

use micro_ir::FunctionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Activate(FunctionId),
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
