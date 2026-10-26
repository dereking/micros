use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use micro_ir::{
    AppImage, FunctionId, FunctionKind, NodeId, StateId, TextSource, TextStyle, UiKind,
    ValidationError, ValueSource, validate,
};
use micro_vm::{HostAccess, NullHost, Value, Vm, VmError};

use crate::{
    Event, EventQueue, MicroUiNode, MicroUiTree, RenderError, RenderPatch, RenderPort, StateStore,
};

const BINDING_BUDGET: u64 = 10_000;

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeError {
    InvalidImage(String),
    Vm(VmError),
    Render(RenderError),
    NotAHandler(FunctionId),
    BindingDidNotReturn(FunctionId),
    TextIsNotString(NodeId),
    ProgressIsNotNumber(NodeId),
    SwitchIsNotBoolean(NodeId),
    InputIsNotString(NodeId),
    SliderIsNotNumber(NodeId),
    CheckboxIsNotBoolean(NodeId),
    SelectionIsNotNumber(NodeId),
    LedIsNotBoolean(NodeId),
    SpinnerIsNotBoolean(NodeId),
    ScaleIsNotNumber(NodeId),
    VisibilityIsNotBoolean(NodeId),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RuntimeError {}

impl From<ValidationError> for RuntimeError {
    fn from(value: ValidationError) -> Self {
        Self::InvalidImage(value.0)
    }
}

impl From<VmError> for RuntimeError {
    fn from(value: VmError) -> Self {
        Self::Vm(value)
    }
}

impl From<RenderError> for RuntimeError {
    fn from(value: RenderError) -> Self {
        Self::Render(value)
    }
}

pub struct Runtime<R> {
    image: AppImage,
    state: StateStore,
    events: EventQueue,
    renderer: R,
    host: Box<dyn HostAccess>,
    event_budget: u64,
    dependencies: BTreeMap<FunctionId, BTreeSet<StateId>>,
    binding_values: BTreeMap<FunctionId, Value>,
}

impl<R: RenderPort> Runtime<R> {
    pub fn new(image: AppImage, renderer: R, event_budget: u64) -> Result<Self, RuntimeError> {
        Self::new_with_host(image, renderer, event_budget, Box::new(NullHost))
    }

    /// Construct with a host accessor so `HostCall` instructions can read host
    /// values, run host actions, and record async requests.
    pub fn new_with_host(
        image: AppImage,
        renderer: R,
        event_budget: u64,
        host: Box<dyn HostAccess>,
    ) -> Result<Self, RuntimeError> {
        validate(&image)?;
        let state = StateStore::from_image(&image);
        let mut runtime = Self {
            image,
            state,
            events: EventQueue::default(),
            renderer,
            host,
            event_budget,
            dependencies: BTreeMap::new(),
            binding_values: BTreeMap::new(),
        };

        let binding_ids: Vec<_> = runtime
            .image
            .functions
            .iter()
            .enumerate()
            .filter_map(|(index, function)| {
                matches!(function.kind, FunctionKind::Binding(_))
                    .then_some(FunctionId(index as u32))
            })
            .collect();
        for id in binding_ids {
            runtime.evaluate_binding(id)?;
        }
        let tree = runtime.materialize_tree()?;
        runtime.renderer.create_tree(&tree)?;
        runtime.apply_initial_visibility()?;
        Ok(runtime)
    }

    pub fn enqueue(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn tick(&mut self) -> Result<bool, RuntimeError> {
        let Some(event) = self.events.pop() else {
            return Ok(false);
        };
        // The DOM input owns the text while the user types, so the binding value
        // cached at boot can go stale. Remember what the user actually entered
        // before running the handler; otherwise a later profile switch back to
        // the same (empty) value would skip the patch and keep showing the old
        // user's text.
        if let Event::InputChanged(handler, text) = &event {
            self.remember_typed_input(*handler, text);
        }
        let (function_id, argument) = match event {
            Event::Activate(id) => (id, None),
            Event::InputChanged(id, text) => (id, Some(Value::String(text))),
            Event::SliderChanged(id, value) => (id, Some(Value::Number(value))),
            Event::CheckedChanged(id, checked) => (id, Some(Value::Bool(checked))),
            Event::SelectionChanged(id, index) => (id, Some(Value::Number(index))),
            Event::HostResult(id, value) => (id, Some(value)),
        };
        if !matches!(
            self.image.functions.get(function_id.0 as usize),
            Some(micro_ir::Function {
                kind: FunctionKind::Handler(_),
                ..
            })
        ) {
            return Err(RuntimeError::NotAHandler(function_id));
        }

        let result = Vm::new(&self.image, &mut self.state).invoke_with_host(
            function_id,
            argument,
            self.event_budget,
            &mut *self.host,
        );
        self.flush_changed_bindings()?;
        result.map_err(RuntimeError::Vm)?;
        Ok(true)
    }

    /// Drain async host completions and re-enqueue them as `HostResult`
    /// events so their 1-arg callback handlers run on the next tick. The
    /// platform host should call this after each tick loop.
    pub fn enqueue_host_results(&mut self) {
        for (handler, value) in self.host.drain_results() {
            self.events.push(Event::HostResult(handler, value));
        }
    }

    pub fn renderer(&self) -> &R {
        &self.renderer
    }

    pub fn renderer_mut(&mut self) -> &mut R {
        &mut self.renderer
    }

    /// Ask the host to release App-owned resources before the runtime is
    /// dropped. Process-wide state (such as background music) stays alive.
    pub fn dispose_host(&mut self) {
        self.host.dispose();
    }

    pub fn state(&self, id: StateId) -> Option<&Value> {
        self.state.get(id)
    }

    /// Apply the initial state of `ui.show` bindings after the native/web
    /// tree exists. Hiding at construction (rather than waiting for the first
    /// state change) avoids a frame where study-only buttons flash on screen.
    fn apply_initial_visibility(&mut self) -> Result<(), RuntimeError> {
        let mut patches = Vec::new();
        for node in &self.image.nodes {
            if node.kind != UiKind::Button {
                continue;
            }
            let visible = match node.value {
                Some(ValueSource::Binding(binding)) => match self.binding_values.get(&binding) {
                    Some(Value::Bool(visible)) => *visible,
                    _ => return Err(RuntimeError::VisibilityIsNotBoolean(node.id)),
                },
                Some(ValueSource::Constant(index)) => {
                    match self.image.constants.get(index as usize) {
                        Some(micro_ir::Constant::Bool(visible)) => *visible,
                        _ => return Err(RuntimeError::VisibilityIsNotBoolean(node.id)),
                    }
                }
                None => continue,
            };
            patches.push(RenderPatch::SetVisible {
                node: node.id,
                visible,
            });
        }
        if !patches.is_empty() {
            self.renderer.apply(&patches)?;
        }
        Ok(())
    }

    /// Keep an input's bound value in sync with the text the user typed. The
    /// node's `on_click` slot holds the `onChange` handler id.
    fn remember_typed_input(&mut self, handler: FunctionId, text: &str) {
        let binding = self
            .image
            .nodes
            .iter()
            .find(|node| node.kind == UiKind::Input && node.on_click == Some(handler))
            .and_then(|node| match node.value {
                Some(ValueSource::Binding(binding)) => Some(binding),
                _ => None,
            });
        if let Some(binding) = binding {
            self.binding_values
                .insert(binding, Value::String(text.to_owned()));
        }
    }

    fn evaluate_binding(&mut self, function_id: FunctionId) -> Result<Value, RuntimeError> {
        self.state.begin_tracking();
        let execution = Vm::new(&self.image, &mut self.state)
            .invoke_with_host(function_id, None, BINDING_BUDGET, &mut *self.host)
            .map_err(RuntimeError::Vm);
        let reads = self.state.finish_tracking();
        let value = execution?
            .value
            .ok_or(RuntimeError::BindingDidNotReturn(function_id))?;
        self.dependencies.insert(function_id, reads);
        self.binding_values.insert(function_id, value.clone());
        Ok(value)
    }

    fn flush_changed_bindings(&mut self) -> Result<(), RuntimeError> {
        let changed = self.state.take_changed();
        let dirty: BTreeSet<_> = self
            .dependencies
            .iter()
            .filter_map(|(binding, reads)| {
                reads
                    .iter()
                    .any(|state| changed.contains(state))
                    .then_some(*binding)
            })
            .collect();
        self.reconcile_bindings(dirty)
    }

    /// Re-evaluate the given bindings and apply render patches for any whose
    /// value changed. Bindings whose value is unchanged emit no patch, so this
    /// is safe to call even when most bindings are unaffected.
    fn reconcile_bindings(&mut self, bindings: BTreeSet<FunctionId>) -> Result<(), RuntimeError> {
        let mut patches = Vec::new();
        for binding in bindings {
            let previous = self.binding_values.get(&binding).cloned();
            let value = self.evaluate_binding(binding)?;
            if previous.as_ref() == Some(&value) {
                continue;
            }
            for node in &self.image.nodes {
                if node.text == Some(TextSource::Binding(binding)) {
                    let Value::String(text) = &value else {
                        continue;
                    };
                    if node.kind == UiKind::Icon {
                        patches.push(RenderPatch::SetIcon {
                            node: node.id,
                            name: text.clone(),
                        });
                    } else {
                        patches.push(RenderPatch::SetText {
                            node: node.id,
                            text: text.clone(),
                        });
                    }
                }
                if node.value == Some(ValueSource::Binding(binding)) {
                    match node.kind {
                        UiKind::Button => {
                            let Value::Bool(visible) = &value else {
                                return Err(RuntimeError::VisibilityIsNotBoolean(node.id));
                            };
                            patches.push(RenderPatch::SetVisible {
                                node: node.id,
                                visible: *visible,
                            });
                        }
                        UiKind::Progress => {
                            let Value::Number(fraction) = &value else {
                                return Err(RuntimeError::ProgressIsNotNumber(node.id));
                            };
                            patches.push(RenderPatch::SetProgress {
                                node: node.id,
                                fraction: fraction.clamp(0.0, 1.0),
                            });
                        }
                        UiKind::Switch | UiKind::Checkbox => {
                            let Value::Bool(checked) = &value else {
                                return Err(RuntimeError::SwitchIsNotBoolean(node.id));
                            };
                            patches.push(RenderPatch::SetChecked {
                                node: node.id,
                                checked: *checked,
                            });
                        }
                        UiKind::Input => {
                            let Value::String(text) = &value else {
                                return Err(RuntimeError::InputIsNotString(node.id));
                            };
                            patches.push(RenderPatch::SetInputText {
                                node: node.id,
                                text: text.clone(),
                            });
                        }
                        UiKind::Slider => {
                            let Value::Number(value) = &value else {
                                return Err(RuntimeError::SliderIsNotNumber(node.id));
                            };
                            patches.push(RenderPatch::SetSliderValue {
                                node: node.id,
                                value: *value,
                            });
                        }
                        UiKind::Dropdown | UiKind::Roller => {
                            let Value::Number(index) = &value else {
                                return Err(RuntimeError::SelectionIsNotNumber(node.id));
                            };
                            patches.push(RenderPatch::SetSelectionValue {
                                node: node.id,
                                index: *index,
                            });
                        }
                        UiKind::Led => {
                            let Value::Bool(on) = &value else {
                                return Err(RuntimeError::LedIsNotBoolean(node.id));
                            };
                            patches.push(RenderPatch::SetLed {
                                node: node.id,
                                on: *on,
                            });
                        }
                        UiKind::Spinner => {
                            let Value::Bool(active) = &value else {
                                return Err(RuntimeError::SpinnerIsNotBoolean(node.id));
                            };
                            patches.push(RenderPatch::SetSpinner {
                                node: node.id,
                                active: *active,
                            });
                        }
                        UiKind::Scale => {
                            let Value::Number(value) = &value else {
                                return Err(RuntimeError::ScaleIsNotNumber(node.id));
                            };
                            patches.push(RenderPatch::SetScaleValue {
                                node: node.id,
                                value: *value,
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
        if !patches.is_empty() {
            self.renderer.apply(&patches)?;
        }
        Ok(())
    }

    /// Re-evaluate every binding so the UI reflects host values that changed
    /// outside the VM — e.g. the ESP32 Wi-Fi radio reaches "connected" after
    /// the shell mounted. The platform host calls this when it observes a host
    /// value the App reads has transitioned. Bindings whose value is unchanged
    /// emit no patches, so a reconcile is a cheap no-op when nothing moved.
    pub fn reconcile(&mut self) -> Result<(), RuntimeError> {
        let all: BTreeSet<_> = self.binding_values.keys().copied().collect();
        self.reconcile_bindings(all)
    }

    fn materialize_tree(&self) -> Result<MicroUiTree, RuntimeError> {
        let mut nodes = Vec::with_capacity(self.image.nodes.len());
        for node in &self.image.nodes {
            let text = match node.text {
                None => String::new(),
                Some(TextSource::Constant(id)) => match self.image.constants.get(id as usize) {
                    Some(micro_ir::Constant::String(text)) => text.clone(),
                    _ => return Err(RuntimeError::TextIsNotString(node.id)),
                },
                Some(TextSource::Binding(id)) => match self.binding_values.get(&id) {
                    Some(Value::String(text)) => text.clone(),
                    _ => return Err(RuntimeError::TextIsNotString(node.id)),
                },
            };
            let value = match node.value {
                None => None,
                Some(ValueSource::Constant(id)) => {
                    self.image.constants.get(id as usize).map(Value::from)
                }
                Some(ValueSource::Binding(id)) => self.binding_values.get(&id).cloned(),
            };
            let value = match node.kind {
                UiKind::Progress => match value {
                    Some(Value::Number(fraction)) => Some(Value::Number(fraction.clamp(0.0, 1.0))),
                    _ => return Err(RuntimeError::ProgressIsNotNumber(node.id)),
                },
                UiKind::Switch => match value {
                    Some(Value::Bool(checked)) => Some(Value::Bool(checked)),
                    _ => return Err(RuntimeError::SwitchIsNotBoolean(node.id)),
                },
                UiKind::Slider => match value {
                    Some(Value::Number(value)) => Some(Value::Number(value)),
                    _ => return Err(RuntimeError::SliderIsNotNumber(node.id)),
                },
                UiKind::Checkbox => match value {
                    Some(Value::Bool(checked)) => Some(Value::Bool(checked)),
                    _ => return Err(RuntimeError::CheckboxIsNotBoolean(node.id)),
                },
                UiKind::Dropdown | UiKind::Roller => match value {
                    Some(Value::Number(index)) => Some(Value::Number(index)),
                    _ => return Err(RuntimeError::SelectionIsNotNumber(node.id)),
                },
                UiKind::Led | UiKind::Spinner => match value {
                    Some(Value::Bool(flag)) => Some(Value::Bool(flag)),
                    _ => return Err(RuntimeError::LedIsNotBoolean(node.id)),
                },
                UiKind::Scale => match value {
                    Some(Value::Number(value)) => Some(Value::Number(value)),
                    _ => return Err(RuntimeError::ScaleIsNotNumber(node.id)),
                },
                _ => value,
            };
            let options = node
                .options
                .iter()
                .map(|option| match self.image.constants.get(*option as usize) {
                    Some(micro_ir::Constant::String(text)) => Ok(text.clone()),
                    _ => Err(RuntimeError::TextIsNotString(node.id)),
                })
                .collect::<Result<Vec<_>, _>>()?;
            nodes.push(MicroUiNode {
                id: node.id,
                kind: node.kind,
                children: node.children.clone(),
                text,
                value,
                on_click: node.on_click,
                text_style: node
                    .text_style
                    .or_else(|| TextStyle::default_for(node.kind)),
                range: node.range,
                options,
                layout: node.layout,
            });
        }
        Ok(MicroUiTree {
            nodes,
            root: self.image.root,
        })
    }
}
