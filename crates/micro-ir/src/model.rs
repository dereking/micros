use std::collections::VecDeque;
use std::fmt;

use crate::Instruction;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);
    };
}

id_type!(StateId);
id_type!(FunctionId);
id_type!(BindingId);
id_type!(HandlerId);
id_type!(NodeId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    Number,
    String,
    Bool,
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Number(f64),
    String(String),
    Bool(bool),
    Null,
}

impl Constant {
    pub fn scalar_type(&self) -> ScalarType {
        match self {
            Self::Number(_) => ScalarType::Number,
            Self::String(_) => ScalarType::String,
            Self::Bool(_) => ScalarType::Bool,
            Self::Null => ScalarType::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDecl {
    pub ty: ScalarType,
    pub initial: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Init,
    Binding(BindingId),
    Handler(HandlerId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub kind: FunctionKind,
    /// Number of arguments the function accepts from the runtime when invoked
    /// as a handler. Only meaningful for `Handler`; Init and Binding functions
    /// always use `0` (a Binding returns its value via `Return`, not via
    /// args). `1` is reserved for `ui.input` `onChange(s)` handlers, where
    /// the runtime passes the new text and the handler reads it with the
    /// `LoadArg` VM instruction.
    pub arg_count: u8,
    pub locals: u16,
    pub max_stack: u16,
    pub code: Vec<Instruction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiKind {
    Column,
    Text,
    Button,
    Row,
    Progress,
    Switch,
    /// Editable text field. The current text lives in `value` (constant or
    /// binding) and the optional placeholder lives in `text` as a constant
    /// string. The optional `on_click` handler is a 1-arg handler that
    /// receives the new text.
    Input,
    /// Draggable numeric input (LVGL slider). The current position lives in
    /// `value`; the optional `range` gives (min, max) in value units; the
    /// optional `on_click` handler is a 1-arg handler receiving the new value.
    Slider,
    /// Boolean toggle with a text label (LVGL checkbox). `value` holds the
    /// checked state; `text` holds the label; the optional `on_click` handler
    /// is a 1-arg handler receiving the new checked state.
    Checkbox,
    /// Selection list (LVGL dropdown). `options` lists the choice strings
    /// (as interned constants); `value` holds the selected index; the
    /// optional `on_click` handler receives the newly selected index.
    Dropdown,
    /// Wheel picker (LVGL roller). Same shape as Dropdown: `options` +
    /// `value` (index) + optional 1-arg `on_click`.
    Roller,
    /// Status indicator (LVGL led). `value` holds a Boolean on/off.
    Led,
    /// Loading spinner. `value` holds a Boolean visible/hidden.
    Spinner,
    /// Read-only gauge (LVGL scale). `value` holds the needle value; the
    /// optional `range` gives (min, max).
    Scale,
    /// Clickable-row container (LVGL list). Children are Button rows; each
    /// carries its own text and onClick handler.
    List,
    /// Tabbed container (LVGL tabview). `options` holds the tab titles (as
    /// interned strings); children are the tab content nodes, one per tab in
    /// the same order.
    Tabview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSource {
    Constant(u32),
    Binding(FunctionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueSource {
    Constant(u32),
    Binding(FunctionId),
}

/// The host-provided capabilities an App can call. Each maps to one of the
/// `device.*` / `net.*` SDK functions; hosts that cannot service a call (e.g.
/// a pure simulator that has not wired the value yet) return a simulated
/// scalar via `HostAccess`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostCallKind {
    /// `device.name()` → App name string.
    DeviceName,
    /// `device.chip()` → chip model string (e.g. `"ESP32-S3"`).
    DeviceChip,
    /// `device.flashBytes()` → flash size in bytes.
    DeviceFlashBytes,
    /// `device.psramBytes()` → PSRAM size in bytes.
    DevicePsramBytes,
    /// `device.resetReason()` → reset reason string (e.g. `"power-on"`).
    DeviceResetReason,
    /// `device.backlight()` → current backlight level (0..4).
    DeviceBacklight,
    /// `device.setBacklight(level)` — fire-and-forget backlight action.
    DeviceSetBacklight,
    /// `device.gpioSetup(pin, mode)` — configure a GPIO as `"in"`, `"out"`,
    /// `"in-pullup"` or `"in-pulldown"`.
    DeviceGpioSetup,
    /// `device.gpioWrite(pin, level)` — set an output GPIO high (1) / low (0).
    DeviceGpioWrite,
    /// `device.gpioRead(pin)` → current input level (0 or 1).
    DeviceGpioRead,
    /// `net.wifiState()` → `"off"|"connecting"|"connected"|"error"`.
    NetWifiState,
    /// `net.wifiSsid()` → connected SSID string (empty when not connected).
    NetWifiSsid,
    /// `net.wifiConnect(ssid, password)` — fire-and-forget connect action.
    NetWifiConnect,
    /// `net.wifiDisconnect()` — fire-and-forget disconnect action.
    NetWifiDisconnect,
    /// `net.scanWifi(onResult)` — async; host calls back with a `\n`-joined
    /// SSID list.
    NetScanWifi,
    /// `net.wifiApName(i)` → SSID of the AP at scan index `i` from the most
    /// recent scan ("" when the scan is empty or `i` is out of range).
    NetWifiApName,
    /// `net.httpGet(url, onResult)` — async; host calls back with the response
    /// body string.
    NetHttpGet,
    /// `net.httpRequest(method, url, body, onResult)` — async; like httpGet but
    /// with an explicit HTTP method (GET/POST/PUT/DELETE/HEAD…) and a request
    /// body.
    NetHttpRequest,
    /// `os.appName(i)` → display name of the installed app at index `i`
    /// (empty when no app is installed at that index).
    OsAppName,
    /// `os.appIcon(i)` → single glyph of the installed app at index `i`.
    OsAppIcon,
    /// `os.launchIndex(i)` — boot the installed app at index `i` (the kernel
    /// tears down the current shell runtime and boots that app).
    OsLaunchIndex,
    /// `os.goBack()` — exit the current app and return to the shell.
    OsGoBack,
    /// `os.delay(ms, onResult)` — async; the host calls back after `ms`
    /// milliseconds (drives the shell's polling heartbeat).
    OsDelay,
}

/// A single host call referenced by `Instruction::HostCall`. Sync reads carry
/// a `result_kind`; actions carry none; async requests carry a 1-arg
/// `callback` handler that the host invokes with the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRequest {
    pub kind: HostCallKind,
    /// Argument types in source order (0..=3 for the current SDK surface).
    pub arg_kinds: Vec<ScalarType>,
    /// Async requests: the 1-arg Handler the host invokes with the result.
    pub callback: Option<FunctionId>,
    /// Sync reads: the scalar type the host returns.
    pub result_kind: Option<ScalarType>,
}

impl HostRequest {
    pub fn sync(
        kind: HostCallKind,
        arg_kinds: Vec<ScalarType>,
        result_kind: Option<ScalarType>,
    ) -> Self {
        Self {
            kind,
            arg_kinds,
            callback: None,
            result_kind,
        }
    }

    pub fn async_request(kind: HostCallKind, arg_kinds: Vec<ScalarType>, callback: FunctionId) -> Self {
        Self {
            kind,
            arg_kinds,
            callback: Some(callback),
            result_kind: None,
        }
    }

    pub fn is_async(&self) -> bool {
        self.callback.is_some()
    }
}

/// Per-node layout hints, set via `ui.place(widget, { left, top, width, height,
/// anchor })`.
///
/// Mirrors Delphi: `left`/`top`/`width`/`height` are the child's base geometry
/// (position and size), and `anchor` pins edges to the parent's edges — an
/// anchored edge takes priority over the base position for that side, and both
/// opposite edges anchored stretches the child along that axis. An unset
/// `width`/`height` falls back to the content size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutSpec {
    /// Base left position (used when the left edge is not anchored).
    pub left: Option<f64>,
    /// Base top position (used when the top edge is not anchored).
    pub top: Option<f64>,
    /// Base width (used when the horizontal axis is not stretched).
    pub width: Option<f64>,
    /// Base height (used when the vertical axis is not stretched).
    pub height: Option<f64>,
    /// Edge anchors: an offset pins that edge to the parent's edge.
    pub anchor: AnchorSpec,
}

/// Edge anchors for `LayoutSpec`. Each offset pins the child's corresponding
/// side to the parent's side at that distance; both opposite edges set
/// stretches the child along that axis. Higher priority than the base
/// left/top/width/height.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AnchorSpec {
    pub left: Option<f64>,
    pub top: Option<f64>,
    pub right: Option<f64>,
    pub bottom: Option<f64>,
}

impl AnchorSpec {
    pub const fn is_empty(&self) -> bool {
        self.left.is_none() && self.top.is_none() && self.right.is_none() && self.bottom.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    UiSans,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    Regular,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStyle {
    pub family: FontFamily,
    pub size_px: u8,
    pub weight: FontWeight,
    pub line_height_px: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStyleError {
    UnsupportedSize(u8),
    UnsupportedLineHeight {
        size_px: u8,
        line_height_px: u8,
        supported_line_height_px: u8,
    },
}

impl fmt::Display for TextStyleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSize(size_px) => {
                write!(f, "unsupported text size {size_px}px")
            }
            Self::UnsupportedLineHeight {
                size_px,
                line_height_px,
                supported_line_height_px,
            } => write!(
                f,
                "unsupported {line_height_px}px line height for {size_px}px text; use {supported_line_height_px}px"
            ),
        }
    }
}

impl std::error::Error for TextStyleError {}

impl TextStyle {
    pub const UI_SANS_METRICS: [(u8, u8); 5] = [(12, 14), (14, 18), (18, 24), (24, 32), (32, 40)];
    pub const DEFAULT_TEXT: Self = Self {
        family: FontFamily::UiSans,
        size_px: 24,
        weight: FontWeight::Regular,
        line_height_px: 32,
    };
    pub const DEFAULT_BUTTON: Self = Self {
        family: FontFamily::UiSans,
        size_px: 14,
        weight: FontWeight::Regular,
        line_height_px: 18,
    };

    pub fn new(
        family: FontFamily,
        size_px: u8,
        weight: FontWeight,
        line_height_px: u8,
    ) -> Result<Self, TextStyleError> {
        let Some((_, supported_line_height_px)) = Self::UI_SANS_METRICS
            .iter()
            .copied()
            .find(|(supported_size_px, _)| *supported_size_px == size_px)
        else {
            return Err(TextStyleError::UnsupportedSize(size_px));
        };
        if line_height_px != supported_line_height_px {
            return Err(TextStyleError::UnsupportedLineHeight {
                size_px,
                line_height_px,
                supported_line_height_px,
            });
        }
        Ok(Self {
            family,
            size_px,
            weight,
            line_height_px,
        })
    }

    pub fn ui_sans(
        size_px: u8,
        weight: FontWeight,
        line_height_px: u8,
    ) -> Result<Self, TextStyleError> {
        Self::new(FontFamily::UiSans, size_px, weight, line_height_px)
    }

    pub const fn default_for(kind: UiKind) -> Option<Self> {
        match kind {
            UiKind::Column
            | UiKind::Row
            | UiKind::Progress
            | UiKind::Switch
            | UiKind::Slider
            | UiKind::Checkbox
            | UiKind::Dropdown
            | UiKind::Roller
            | UiKind::Led
            | UiKind::Spinner
            | UiKind::Scale
            | UiKind::List
            | UiKind::Tabview => None,
            UiKind::Text | UiKind::Input => Some(Self::DEFAULT_TEXT),
            UiKind::Button => Some(Self::DEFAULT_BUTTON),
        }
    }

    fn validate(self) -> Result<(), TextStyleError> {
        Self::new(self.family, self.size_px, self.weight, self.line_height_px).map(|_| ())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiNodeSpec {
    pub id: NodeId,
    pub kind: UiKind,
    pub children: Vec<NodeId>,
    pub text: Option<TextSource>,
    pub value: Option<ValueSource>,
    pub on_click: Option<FunctionId>,
    pub text_style: Option<TextStyle>,
    /// Optional (min, max) range in value units, used by value widgets that
    /// accept a range (e.g. Slider). `None` means the renderer default.
    pub range: Option<(f64, f64)>,
    /// Interned string constants for selection widgets (Dropdown/Roller).
    pub options: Vec<u32>,
    /// Optional Delphi-style layout hint from `ui.place`.
    pub layout: Option<LayoutSpec>,
}

/// App manifest metadata, declared in the app source as
/// `app({ id, name, icon })` and serialized into the MBC's metadata section.
/// The OS launcher reads `id` (to launch), `name` (to display) and `icon`
/// (a single display glyph) without decoding the whole app.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppMetadata {
    /// Stable ASCII identifier used to launch the app (e.g. `"counter"`).
    pub id: String,
    /// Human-readable display name shown under the launcher icon.
    pub name: String,
    /// Single glyph rendered as the launcher icon (ASCII/kept in the LVGL font).
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppImage {
    pub constants: Vec<Constant>,
    pub states: Vec<StateDecl>,
    pub functions: Vec<Function>,
    pub nodes: Vec<UiNodeSpec>,
    pub host_requests: Vec<HostRequest>,
    /// App manifest (`app({ id, name, icon })`).
    pub metadata: AppMetadata,
    pub root: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError(pub String);

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ValidationError {}

fn invalid(message: impl Into<String>) -> ValidationError {
    ValidationError(message.into())
}

pub fn validate(image: &AppImage) -> Result<(), ValidationError> {
    if image.metadata.id.is_empty() || image.metadata.name.is_empty() {
        return Err(invalid("app metadata requires a non-empty id and name"));
    }
    if image.metadata.icon.chars().count() != 1 {
        return Err(invalid("app metadata icon must be a single glyph"));
    }
    if image.nodes.is_empty() {
        return Err(invalid("UI tree is empty"));
    }
    if image.root.0 as usize >= image.nodes.len() {
        return Err(invalid("root node is out of range"));
    }

    for (index, state) in image.states.iter().enumerate() {
        let constant = image
            .constants
            .get(state.initial as usize)
            .ok_or_else(|| invalid(format!("state {index} has an invalid initial constant")))?;
        if constant.scalar_type() != state.ty {
            return Err(invalid(format!(
                "state {index} initial type does not match"
            )));
        }
    }

    for (index, function) in image.functions.iter().enumerate() {
        validate_function(image, index, function)?;
    }

    for (index, request) in image.host_requests.iter().enumerate() {
        if request.arg_kinds.len() > 3 {
            return Err(invalid(format!(
                "host request {index} has too many arguments"
            )));
        }
        if let Some(callback) = request.callback {
            match image.functions.get(callback.0 as usize) {
                Some(Function {
                    kind: FunctionKind::Handler(_),
                    arg_count: 1,
                    ..
                }) => {}
                _ => {
                    return Err(invalid(format!(
                        "host request {index} has an invalid async callback"
                    )));
                }
            }
        }
        if request
            .result_kind
            .is_some_and(|result| !matches!(result, ScalarType::Number | ScalarType::String))
        {
            return Err(invalid(format!(
                "host request {index} has a non-scalar result type"
            )));
        }
    }

    for (index, node) in image.nodes.iter().enumerate() {
        if node.id.0 as usize != index {
            return Err(invalid(format!("node id {} is not canonical", node.id.0)));
        }
        for child in &node.children {
            if child.0 as usize >= image.nodes.len() {
                return Err(invalid(format!("node {index} has an invalid child")));
            }
        }
        if let Some(text) = node.text {
            match text {
                TextSource::Constant(id) if id as usize >= image.constants.len() => {
                    return Err(invalid(format!(
                        "node {index} has an invalid text constant"
                    )));
                }
                TextSource::Binding(id) => match image.functions.get(id.0 as usize) {
                    Some(Function {
                        kind: FunctionKind::Binding(_),
                        ..
                    }) => {}
                    _ => return Err(invalid(format!("node {index} has an invalid binding"))),
                },
                TextSource::Constant(_) => {}
            }
        }
        if let Some(value) = node.value {
            match value {
                ValueSource::Constant(id) if id as usize >= image.constants.len() => {
                    return Err(invalid(format!(
                        "node {index} has an invalid value constant"
                    )));
                }
                ValueSource::Binding(id) => match image.functions.get(id.0 as usize) {
                    Some(Function {
                        kind: FunctionKind::Binding(_),
                        ..
                    }) => {}
                    _ => return Err(invalid(format!("node {index} has an invalid binding"))),
                },
                ValueSource::Constant(_) => {}
            }
        }
        if let Some(id) = node.on_click {
            match image.functions.get(id.0 as usize) {
                Some(Function {
                    kind: FunctionKind::Handler(_),
                    ..
                }) => {}
                _ => {
                    return Err(invalid(format!(
                        "node {index} has an invalid click handler"
                    )));
                }
            }
        }
        if let Some(style) = node.text_style {
            style.validate().map_err(|error| {
                invalid(format!("node {index} has an invalid text style: {error}"))
            })?;
        }
    }
    Ok(())
}

fn validate_function(
    image: &AppImage,
    index: usize,
    function: &Function,
) -> Result<(), ValidationError> {
    if function.code.is_empty() {
        return Err(invalid(format!("function {index} is empty")));
    }
    let mut depths = vec![None; function.code.len()];
    let mut queue = VecDeque::from([(0_usize, 0_i32)]);
    let mut observed_max = 0_i32;

    while let Some((pc, depth)) = queue.pop_front() {
        if pc >= function.code.len() {
            return Err(invalid(format!("function {index} falls off the end")));
        }
        if let Some(previous) = depths[pc] {
            if previous != depth {
                return Err(invalid(format!(
                    "function {index} has inconsistent stack depth"
                )));
            }
            continue;
        }
        depths[pc] = Some(depth);
        let instruction = &function.code[pc];
        validate_operand(image, function, index, instruction)?;
        let (required, delta) = stack_effect(image, instruction);
        if depth < required {
            return Err(invalid(format!("function {index} underflows its stack")));
        }
        let next_depth = depth + delta;
        observed_max = observed_max.max(next_depth);

        match instruction {
            Instruction::Return => {
                let expected = match function.kind {
                    FunctionKind::Binding(_) => 1,
                    FunctionKind::Init | FunctionKind::Handler(_) => 0,
                };
                if depth != expected {
                    return Err(invalid(format!(
                        "function {index} returns with wrong stack depth"
                    )));
                }
            }
            Instruction::Jump(target) => {
                push_target(index, &function.code, &mut queue, *target, next_depth)?
            }
            Instruction::JumpIfFalse(target) => {
                push_target(index, &function.code, &mut queue, *target, next_depth)?;
                queue.push_back((pc + 1, next_depth));
            }
            _ => queue.push_back((pc + 1, next_depth)),
        }
    }

    if observed_max > i32::from(function.max_stack) {
        return Err(invalid(format!(
            "function {index} declares too little stack"
        )));
    }
    Ok(())
}

fn push_target(
    index: usize,
    code: &[Instruction],
    queue: &mut VecDeque<(usize, i32)>,
    target: u32,
    depth: i32,
) -> Result<(), ValidationError> {
    if target as usize >= code.len() {
        return Err(invalid(format!("function {index} has an invalid jump")));
    }
    queue.push_back((target as usize, depth));
    Ok(())
}

fn validate_operand(
    image: &AppImage,
    function: &Function,
    index: usize,
    instruction: &Instruction,
) -> Result<(), ValidationError> {
    match instruction {
        Instruction::Const(id) if *id as usize >= image.constants.len() => {
            Err(invalid(format!("function {index} has an invalid constant")))
        }
        Instruction::LoadState(id) | Instruction::StoreState(id)
            if id.0 as usize >= image.states.len() =>
        {
            Err(invalid(format!("function {index} has an invalid state")))
        }
        Instruction::LoadLocal(id) | Instruction::StoreLocal(id) if *id >= function.locals => {
            Err(invalid(format!("function {index} has an invalid local")))
        }
        Instruction::LoadArg if function.arg_count < 1 => {
            Err(invalid(format!("function {index} uses LoadArg without an argument")))
        }
        Instruction::HostCall(idx) if *idx as usize >= image.host_requests.len() => {
            Err(invalid(format!("function {index} has an invalid host call")))
        }
        _ => Ok(()),
    }
}

fn stack_effect(image: &AppImage, instruction: &Instruction) -> (i32, i32) {
    match instruction {
        Instruction::Const(_)
        | Instruction::LoadLocal(_)
        | Instruction::LoadState(_)
        | Instruction::LoadArg => (0, 1),
        Instruction::StoreLocal(_) | Instruction::StoreState(_) | Instruction::Pop => (1, -1),
        Instruction::Add
        | Instruction::Sub
        | Instruction::Mul
        | Instruction::Div
        | Instruction::Mod
        | Instruction::Eq
        | Instruction::Lt
        | Instruction::Gt
        | Instruction::Concat => (2, -1),
        Instruction::Not | Instruction::ToString => (1, 0),
        Instruction::JumpIfFalse(_) => (1, -1),
        Instruction::Dup => (1, 1),
        Instruction::Jump(_) | Instruction::Return => (0, 0),
        /* HostCall pops its argument count; a sync read pushes one value back
         * (net +1), actions and async requests push nothing (net −args). */
        Instruction::HostCall(idx) => {
            let arg_count = image
                .host_requests
                .get(*idx as usize)
                .map_or(0, |request| request.arg_kinds.len());
            let result = image
                .host_requests
                .get(*idx as usize)
                .is_some_and(|request| request.result_kind.is_some());
            let required = arg_count as i32;
            let delta = i32::from(result) - required;
            (required, delta)
        }
    }
}
