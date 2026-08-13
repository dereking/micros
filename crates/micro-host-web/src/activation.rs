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
