use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use micro_ir::FunctionId;

#[derive(Clone, Default)]
pub struct ActivationQueue(Rc<RefCell<VecDeque<FunctionId>>>);

impl ActivationQueue {
    pub fn push(&self, handler: FunctionId) {
        self.0.borrow_mut().push_back(handler);
    }

    pub fn pop(&mut self) -> Option<FunctionId> {
        self.0.borrow_mut().pop_front()
    }
}

/// Queue of `ui.input` onChange events: the handler id plus the new text.
#[derive(Clone, Default)]
pub struct InputChangeQueue(Rc<RefCell<VecDeque<(FunctionId, String)>>>);

impl InputChangeQueue {
    pub fn push(&self, handler: FunctionId, text: String) {
        self.0.borrow_mut().push_back((handler, text));
    }

    pub fn pop(&mut self) -> Option<(FunctionId, String)> {
        self.0.borrow_mut().pop_front()
    }
}

/// Queue of `ui.slider` onChange events: the handler id plus the new value.
#[derive(Clone, Default)]
pub struct SliderChangeQueue(Rc<RefCell<VecDeque<(FunctionId, f64)>>>);

impl SliderChangeQueue {
    pub fn push(&self, handler: FunctionId, value: f64) {
        self.0.borrow_mut().push_back((handler, value));
    }

    pub fn pop(&mut self) -> Option<(FunctionId, f64)> {
        self.0.borrow_mut().pop_front()
    }
}
