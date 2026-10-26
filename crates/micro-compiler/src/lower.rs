use std::collections::BTreeMap;

use micro_ir::{
    AnchorSpec, AppImage, AppMetadata, BindingId, Capability, Constant, FontFamily, FontWeight,
    Function, FunctionId, FunctionKind, HandlerId, HostCallKind, HostRequest, Instruction,
    LayoutSpec, NodeId, ScalarType, StateDecl, StateId, TextSource, TextStyle, UiKind, UiNodeSpec,
    ValueSource, validate,
};
use swc_common::{SourceMap, Span, Spanned, sync::Lrc};
use swc_ecma_ast::{
    ArrowExpr, AssignOp, AssignTarget, BinaryOp, BlockStmtOrExpr, Callee, Decl, Expr, Lit,
    MemberExpr, MemberProp, Module, ModuleItem, Pat, Prop, PropName, PropOrSpread,
    SimpleAssignTarget, Stmt, UpdateOp,
};

use crate::{Diagnostic, ParsedProgram, parse::diagnostic_at, parse_validated};

/// `onClick` handler, `textStyle`, tone, and variant of one `ui.icon` call.
type IconOptions<'b> = (Option<&'b ArrowExpr>, Option<TextStyle>, String, String);

pub fn compile_source(path: &str, source: &str) -> Result<AppImage, Vec<Diagnostic>> {
    let ParsedProgram {
        module,
        source_map,
        path,
    } = parse_validated(path, source)?;
    Lowerer::new(&source_map, &path)
        .lower(&module)
        .map_err(|error| vec![error])
}

struct Lowerer<'a> {
    source_map: &'a SourceMap,
    path: &'a str,
    constants: Vec<Constant>,
    states: Vec<StateDecl>,
    state_symbols: BTreeMap<String, (StateId, ScalarType)>,
    functions: Vec<Function>,
    nodes: Vec<UiNodeSpec>,
    host_requests: Vec<micro_ir::HostRequest>,
    metadata: AppMetadata,
    binding_count: u32,
    handler_count: u32,
}

impl<'a> Lowerer<'a> {
    fn new(source_map: &'a Lrc<SourceMap>, path: &'a str) -> Self {
        Self {
            source_map,
            path,
            constants: Vec::new(),
            states: Vec::new(),
            state_symbols: BTreeMap::new(),
            functions: Vec::new(),
            nodes: Vec::new(),
            host_requests: Vec::new(),
            metadata: AppMetadata::default(),
            binding_count: 0,
            handler_count: 0,
        }
    }

    fn lower(mut self, module: &Module) -> Result<AppImage, Diagnostic> {
        for item in &module.body {
            if let ModuleItem::Stmt(Stmt::Decl(Decl::Var(declaration))) = item {
                for declarator in &declaration.decls {
                    let Pat::Ident(name) = &declarator.name else {
                        continue;
                    };
                    let Some(initializer) = &declarator.init else {
                        continue;
                    };
                    if call_name_expr(initializer).as_deref() == Some("state") {
                        self.lower_state(name.id.sym.as_ref(), initializer)?;
                    }
                }
            }
        }

        let mount_argument = module
            .body
            .iter()
            .find_map(|item| {
                let ModuleItem::Stmt(Stmt::Expr(statement)) = item else {
                    return None;
                };
                let Expr::Call(call) = &*statement.expr else {
                    return None;
                };
                (call_name(call).as_deref() == Some("ui.mount")).then(|| &*call.args[0].expr)
            })
            .ok_or_else(|| self.error(module.span, "MTS010", "ui.mount call is missing"))?;
        // Parse the manifest before lowering so every host call can be checked
        // against the declared capabilities with its own source span.
        self.metadata = self
            .metadata_from_module(module)
            .unwrap_or_else(|| AppMetadata {
                id: "app".to_owned(),
                name: "App".to_owned(),
                icon: "A".to_owned(),
                capabilities: None,
            });
        let root = self.lower_ui(mount_argument)?;
        let source_map = self.source_map;
        let path = self.path;
        let image = AppImage {
            constants: self.constants,
            states: self.states,
            functions: self.functions,
            nodes: self.nodes,
            host_requests: self.host_requests,
            metadata: self.metadata,
            root,
        };
        validate(&image)
            .map_err(|error| diagnostic_at(source_map, path, module.span, "MTS010", error.0))?;
        Ok(image)
    }

    /// Extract the app manifest from a top-level `app({ id, name, icon })`
    /// call, mirroring the `ui.mount` `find_map` pattern. Returns `None` when
    /// no `app()` call is present (caller supplies a default).
    fn metadata_from_module(&self, module: &Module) -> Option<AppMetadata> {
        let object = module.body.iter().find_map(|item| {
            let ModuleItem::Stmt(Stmt::Expr(statement)) = item else {
                return None;
            };
            let Expr::Call(call) = &*statement.expr else {
                return None;
            };
            if call_name(call).as_deref() != Some("app") {
                return None;
            }
            match &*call.args[0].expr {
                Expr::Object(object) => Some(object),
                _ => None,
            }
        })?;
        let mut metadata = AppMetadata {
            capabilities: Some(Vec::new()),
            ..AppMetadata::default()
        };
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                continue;
            };
            let Prop::KeyValue(property) = &**property else {
                continue;
            };
            let PropName::Ident(name) = &property.key else {
                continue;
            };
            match name.sym.as_ref() {
                "id" | "name" | "icon" => {
                    let Expr::Lit(Lit::Str(value)) = &*property.value else {
                        continue;
                    };
                    match name.sym.as_ref() {
                        "id" => metadata.id = value.value.to_string_lossy().into_owned(),
                        "name" => metadata.name = value.value.to_string_lossy().into_owned(),
                        "icon" => metadata.icon = value.value.to_string_lossy().into_owned(),
                        _ => unreachable!(),
                    }
                }
                "capabilities" => {
                    let Expr::Array(array) = &*property.value else {
                        continue;
                    };
                    let mut capabilities = Vec::new();
                    for element in &array.elems {
                        let Some(element) = element else {
                            continue;
                        };
                        let Expr::Lit(Lit::Str(value)) = &*element.expr else {
                            continue;
                        };
                        if let Some(capability) = Capability::parse(&value.value.to_string_lossy())
                        {
                            capabilities.push(capability);
                        }
                    }
                    metadata.capabilities = Some(capabilities);
                }
                _ => {}
            }
        }
        Some(metadata)
    }

    fn lower_state(&mut self, name: &str, initializer: &Expr) -> Result<(), Diagnostic> {
        let Expr::Call(call) = initializer else {
            unreachable!()
        };
        let constant = literal_constant(&call.args[0].expr).ok_or_else(|| {
            self.error(
                call.args[0].span(),
                "MTS011",
                "state initial value must be a scalar literal",
            )
        })?;
        let ty = constant.scalar_type();
        let initial = self.intern(constant);
        let id = StateId(self.states.len() as u32);
        self.states.push(StateDecl { ty, initial });
        self.state_symbols.insert(name.to_owned(), (id, ty));
        Ok(())
    }

    fn lower_ui(&mut self, expression: &Expr) -> Result<NodeId, Diagnostic> {
        let Expr::Call(call) = expression else {
            return Err(self.error(expression.span(), "MTS012", "UI value must be an ui.* call"));
        };
        match call_name(call).as_deref() {
            Some("ui.place") => {
                let id = self.lower_ui(&call.args[0].expr)?;
                self.nodes[id.0 as usize].layout =
                    Some(self.lower_place_layout(&call.args[1].expr)?);
                Ok(id)
            }
            Some("ui.column") => {
                let id = self.reserve_node(UiKind::Column);
                self.nodes[id.0 as usize].children =
                    self.lower_child_array(&call.args[0].expr, "ui.column")?;
                Ok(id)
            }
            Some("ui.row") => {
                let id = self.reserve_node(UiKind::Row);
                self.nodes[id.0 as usize].children =
                    self.lower_child_array(&call.args[0].expr, "ui.row")?;
                Ok(id)
            }
            Some("ui.surface") => {
                let id = self.reserve_node(UiKind::Surface);
                if let Some(argument) = call.args.first() {
                    let constant = literal_constant(&argument.expr).ok_or_else(|| {
                        self.error(
                            argument.expr.span(),
                            "MTS012",
                            "ui.surface tone must be a string literal",
                        )
                    })?;
                    let Constant::String(tone) = constant else {
                        return Err(self.error(
                            argument.expr.span(),
                            "MTS012",
                            "ui.surface tone must be a string literal",
                        ));
                    };
                    let index = self.intern(Constant::String(tone));
                    self.nodes[id.0 as usize].options.push(index);
                }
                if let Some(options) = call.args.get(1) {
                    let (handler, label) = self.lower_surface_options(&options.expr)?;
                    if let Some(handler) = handler {
                        self.nodes[id.0 as usize].on_click =
                            Some(self.add_function(handler, false)?);
                    }
                    if let Some(label) = label {
                        let index = self.intern(Constant::String(label));
                        self.nodes[id.0 as usize].text = Some(TextSource::Constant(index));
                    }
                }
                Ok(id)
            }
            Some("ui.icon") => {
                let id = self.reserve_node(UiKind::Icon);
                let name = if call_name_expr(&call.args[0].expr).as_deref() == Some("bind") {
                    let Expr::Call(binding) = &*call.args[0].expr else {
                        unreachable!()
                    };
                    let arrow = as_arrow(&binding.args[0].expr).ok_or_else(|| {
                        self.error(binding.args[0].span(), "MTS012", "bind expects an arrow")
                    })?;
                    TextSource::Binding(self.add_function(arrow, true)?)
                } else {
                    let constant = literal_constant(&call.args[0].expr)
                        .filter(|constant| matches!(constant, Constant::String(_)))
                        .ok_or_else(|| {
                            self.error(
                                call.args[0].span(),
                                "MTS012",
                                "ui.icon name must be a string or bind",
                            )
                        })?;
                    if let Constant::String(name) = &constant {
                        self.validate_icon_name(call.args[0].span(), name)?;
                    }
                    TextSource::Constant(self.intern(constant))
                };
                self.nodes[id.0 as usize].text = Some(name);
                let (handler, text_style, tone, variant) = self
                    .lower_icon_options(call.args.get(1).map(|argument| argument.expr.as_ref()))?;
                self.nodes[id.0 as usize].on_click = handler
                    .map(|handler| self.add_function(handler, false))
                    .transpose()?;
                self.nodes[id.0 as usize].text_style =
                    Some(text_style.unwrap_or(TextStyle::DEFAULT_ICON));
                for option in [tone, variant] {
                    let index = self.intern(Constant::String(option));
                    self.nodes[id.0 as usize].options.push(index);
                }
                Ok(id)
            }
            Some("ui.show") => {
                let id = self.lower_ui(&call.args[0].expr)?;
                self.nodes[id.0 as usize].visible =
                    Some(self.lower_value_source(ScalarType::Bool, &call.args[1].expr)?);
                Ok(id)
            }
            Some("ui.progress") => {
                let id = self.reserve_node(UiKind::Progress);
                self.nodes[id.0 as usize].value =
                    Some(self.lower_value_source(ScalarType::Number, &call.args[0].expr)?);
                Ok(id)
            }
            Some("ui.switch") => {
                let id = self.reserve_node(UiKind::Switch);
                self.nodes[id.0 as usize].value =
                    Some(self.lower_value_source(ScalarType::Bool, &call.args[0].expr)?);
                if let Some(options) = call.args.get(1) {
                    let arrow = self.lower_switch_options(&options.expr)?;
                    self.nodes[id.0 as usize].on_click = Some(self.add_function(arrow, false)?);
                }
                Ok(id)
            }
            Some("ui.text") => {
                let id = self.reserve_node(UiKind::Text);
                let source = if call_name_expr(&call.args[0].expr).as_deref() == Some("bind") {
                    let Expr::Call(binding) = &*call.args[0].expr else {
                        unreachable!()
                    };
                    let arrow = as_arrow(&binding.args[0].expr).ok_or_else(|| {
                        self.error(binding.args[0].span(), "MTS012", "bind expects an arrow")
                    })?;
                    TextSource::Binding(self.add_function(arrow, true)?)
                } else {
                    let constant = literal_constant(&call.args[0].expr).ok_or_else(|| {
                        self.error(
                            call.args[0].span(),
                            "MTS012",
                            "ui.text expects a string or bind",
                        )
                    })?;
                    if let Constant::String(value) = &constant {
                        self.validate_literal_glyphs(call.args[0].span(), value)?;
                    }
                    TextSource::Constant(self.intern(constant))
                };
                self.nodes[id.0 as usize].text = Some(source);
                self.nodes[id.0 as usize].text_style = Some(
                    call.args
                        .get(1)
                        .map(|argument| self.lower_text_style(&argument.expr))
                        .transpose()?
                        .unwrap_or(TextStyle::DEFAULT_TEXT),
                );
                Ok(id)
            }
            Some("ui.input") => {
                let id = self.reserve_node(UiKind::Input);
                self.nodes[id.0 as usize].value =
                    Some(self.lower_value_source(ScalarType::String, &call.args[0].expr)?);
                self.nodes[id.0 as usize].text_style = Some(TextStyle::DEFAULT_TEXT);
                if let Some(options) = call.args.get(1) {
                    self.lower_input_options(id, &options.expr)?;
                }
                Ok(id)
            }
            Some("ui.button") => {
                let id = self.reserve_node(UiKind::Button);
                let source = if call_name_expr(&call.args[0].expr).as_deref() == Some("bind") {
                    let Expr::Call(binding) = &*call.args[0].expr else {
                        unreachable!()
                    };
                    let arrow = as_arrow(&binding.args[0].expr).ok_or_else(|| {
                        self.error(binding.args[0].span(), "MTS012", "bind expects an arrow")
                    })?;
                    TextSource::Binding(self.add_function(arrow, true)?)
                } else {
                    let constant = literal_constant(&call.args[0].expr)
                        .filter(|constant| matches!(constant, Constant::String(_)))
                        .ok_or_else(|| {
                            self.error(
                                call.args[0].span(),
                                "MTS012",
                                "button label must be a string or bind",
                            )
                        })?;
                    if let Constant::String(value) = &constant {
                        self.validate_literal_glyphs(call.args[0].span(), value)?;
                    }
                    TextSource::Constant(self.intern(constant))
                };
                self.nodes[id.0 as usize].text = Some(source);
                let (arrow, text_style, variant) = self.lower_button_options(&call.args[1].expr)?;
                self.nodes[id.0 as usize].on_click = Some(self.add_function(arrow, false)?);
                self.nodes[id.0 as usize].text_style =
                    Some(text_style.unwrap_or(TextStyle::DEFAULT_BUTTON));
                if let Some(variant) = variant {
                    let index = self.intern(Constant::String(variant));
                    self.nodes[id.0 as usize].options.push(index);
                }
                Ok(id)
            }
            Some("ui.slider") => {
                let id = self.reserve_node(UiKind::Slider);
                self.nodes[id.0 as usize].value =
                    Some(self.lower_value_source(ScalarType::Number, &call.args[0].expr)?);
                if let Some(options) = call.args.get(1) {
                    self.lower_slider_options(id, &options.expr)?;
                }
                Ok(id)
            }
            Some("ui.dropdown") => {
                let id = self.reserve_node(UiKind::Dropdown);
                self.nodes[id.0 as usize].options =
                    self.lower_string_array(&call.args[0].expr, "ui.dropdown")?;
                self.nodes[id.0 as usize].value =
                    Some(self.lower_value_source(ScalarType::Number, &call.args[1].expr)?);
                if let Some(options) = call.args.get(2) {
                    self.lower_dropdown_options(id, &options.expr)?;
                }
                Ok(id)
            }
            Some("ui.tabview") => {
                let id = self.reserve_node(UiKind::Tabview);
                let (titles, contents) = self.lower_tabview_tabs(&call.args[0].expr)?;
                self.nodes[id.0 as usize].options = titles;
                self.nodes[id.0 as usize].children = contents;
                Ok(id)
            }
            Some("ui.list") => {
                let id = self.reserve_node(UiKind::List);
                let children = self.lower_list_items(&call.args[0].expr)?;
                self.nodes[id.0 as usize].children = children;
                Ok(id)
            }
            Some("ui.led") => {
                let id = self.reserve_node(UiKind::Led);
                self.nodes[id.0 as usize].value =
                    Some(self.lower_value_source(ScalarType::Bool, &call.args[0].expr)?);
                Ok(id)
            }
            Some("ui.spinner") => {
                let id = self.reserve_node(UiKind::Spinner);
                self.nodes[id.0 as usize].value =
                    Some(self.lower_value_source(ScalarType::Bool, &call.args[0].expr)?);
                Ok(id)
            }
            Some("ui.scale") => {
                let id = self.reserve_node(UiKind::Scale);
                self.nodes[id.0 as usize].value =
                    Some(self.lower_value_source(ScalarType::Number, &call.args[0].expr)?);
                if let Some(options) = call.args.get(1) {
                    self.lower_scale_options(id, &options.expr)?;
                }
                Ok(id)
            }
            Some("ui.roller") => {
                let id = self.reserve_node(UiKind::Roller);
                self.nodes[id.0 as usize].options =
                    self.lower_string_array(&call.args[0].expr, "ui.roller")?;
                self.nodes[id.0 as usize].value =
                    Some(self.lower_value_source(ScalarType::Number, &call.args[1].expr)?);
                if let Some(options) = call.args.get(2) {
                    self.lower_selection_options(id, &options.expr, "ui.roller")?;
                }
                Ok(id)
            }
            Some("ui.checkbox") => {
                let id = self.reserve_node(UiKind::Checkbox);
                let label = literal_constant(&call.args[0].expr)
                    .filter(|constant| matches!(constant, Constant::String(_)))
                    .ok_or_else(|| {
                        self.error(
                            call.args[0].span(),
                            "MTS012",
                            "checkbox label must be a string",
                        )
                    })?;
                if let Constant::String(value) = &label {
                    self.validate_literal_glyphs(call.args[0].span(), value)?;
                }
                self.nodes[id.0 as usize].text = Some(TextSource::Constant(self.intern(label)));
                self.nodes[id.0 as usize].value =
                    Some(self.lower_value_source(ScalarType::Bool, &call.args[1].expr)?);
                if let Some(options) = call.args.get(2) {
                    self.lower_checkbox_options(id, &options.expr)?;
                }
                Ok(id)
            }
            _ => Err(self.error(call.span, "MTS012", "unsupported UI call")),
        }
    }

    fn reserve_node(&mut self, kind: UiKind) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(UiNodeSpec {
            id,
            kind,
            children: vec![],
            text: None,
            value: None,
            on_click: None,
            text_style: None,
            range: None,
            options: vec![],
            layout: None,
            visible: None,
        });
        id
    }

    fn lower_child_array(
        &mut self,
        expression: &Expr,
        widget: &str,
    ) -> Result<Vec<NodeId>, Diagnostic> {
        let Expr::Array(children) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                format!("{widget} expects a child array"),
            ));
        };
        let mut child_ids = Vec::new();
        for child in children.elems.iter().flatten() {
            child_ids.push(self.lower_ui(&child.expr)?);
        }
        Ok(child_ids)
    }

    fn lower_value_source(
        &mut self,
        expected: ScalarType,
        expression: &Expr,
    ) -> Result<ValueSource, Diagnostic> {
        if call_name_expr(expression).as_deref() == Some("bind") {
            let Expr::Call(binding) = expression else {
                unreachable!()
            };
            let arrow = as_arrow(&binding.args[0].expr).ok_or_else(|| {
                self.error(binding.args[0].span(), "MTS012", "bind expects an arrow")
            })?;
            Ok(ValueSource::Binding(self.add_function(arrow, true)?))
        } else {
            let constant = literal_constant(expression).ok_or_else(|| {
                self.error(
                    expression.span(),
                    "MTS012",
                    "value must be a scalar literal or bind",
                )
            })?;
            if constant.scalar_type() != expected {
                let expected = match expected {
                    ScalarType::Number => "number",
                    ScalarType::Bool => "boolean",
                    _ => "scalar",
                };
                return Err(self.error(
                    expression.span(),
                    "MTS012",
                    format!("progress/switch value must be a {expected}"),
                ));
            }
            Ok(ValueSource::Constant(self.intern(constant)))
        }
    }

    fn lower_switch_options<'b>(&self, expression: &'b Expr) -> Result<&'b ArrowExpr, Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.switch options must be an object",
            ));
        };
        let mut handler = None;
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.switch options cannot use spread",
                ));
            };
            let Prop::KeyValue(property) = &**property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.switch options must use key-value pairs",
                ));
            };
            let PropName::Ident(name) = &property.key else {
                return Err(self.error(
                    property.key.span(),
                    "MTS012",
                    "ui.switch option names must be identifiers",
                ));
            };
            match name.sym.as_ref() {
                "onToggle" if handler.is_none() => {
                    handler = Some(as_arrow(&property.value).ok_or_else(|| {
                        self.error(
                            property.value.span(),
                            "MTS012",
                            "onToggle arrow is required",
                        )
                    })?);
                }
                "onToggle" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        format!("duplicate ui.switch property `{}`", name.sym),
                    ));
                }
                _ => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS002",
                        format!("unknown ui.switch property `{}`", name.sym),
                    ));
                }
            }
        }
        handler.ok_or_else(|| self.error(expression.span(), "MTS012", "onToggle arrow is required"))
    }

    fn lower_input_options(&mut self, node: NodeId, expression: &Expr) -> Result<(), Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.input options must be an object",
            ));
        };
        let mut saw_change = false;
        let mut saw_placeholder = false;
        let mut saw_style = false;
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.input options cannot use spread",
                ));
            };
            let Prop::KeyValue(property) = &**property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.input options must use key-value pairs",
                ));
            };
            let PropName::Ident(name) = &property.key else {
                return Err(self.error(
                    property.key.span(),
                    "MTS012",
                    "ui.input option names must be identifiers",
                ));
            };
            match name.sym.as_ref() {
                "onChange" if !saw_change => {
                    saw_change = true;
                    let arrow = as_arrow(&property.value).ok_or_else(|| {
                        self.error(
                            property.value.span(),
                            "MTS012",
                            "onChange arrow is required",
                        )
                    })?;
                    self.nodes[node.0 as usize].on_click =
                        Some(self.add_input_function(arrow, ScalarType::String)?);
                }
                "onChange" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        format!("duplicate ui.input property `{}`", name.sym),
                    ));
                }
                "placeholder" if !saw_placeholder => {
                    saw_placeholder = true;
                    let constant = literal_constant(&property.value)
                        .filter(|constant| matches!(constant, Constant::String(_)))
                        .ok_or_else(|| {
                            self.error(
                                property.value.span(),
                                "MTS012",
                                "placeholder must be a string",
                            )
                        })?;
                    self.nodes[node.0 as usize].text =
                        Some(TextSource::Constant(self.intern(constant)));
                }
                "placeholder" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        format!("duplicate ui.input property `{}`", name.sym),
                    ));
                }
                "textStyle" if !saw_style => {
                    saw_style = true;
                    self.nodes[node.0 as usize].text_style =
                        Some(self.lower_text_style(&property.value)?);
                }
                "textStyle" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        format!("duplicate ui.input property `{}`", name.sym),
                    ));
                }
                _ => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS002",
                        format!("unknown ui.input property `{}`", name.sym),
                    ));
                }
            }
        }
        Ok(())
    }

    fn lower_slider_options(&mut self, node: NodeId, expression: &Expr) -> Result<(), Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.slider options must be an object",
            ));
        };
        let mut saw_change = false;
        let mut min_value = None;
        let mut max_value = None;
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.slider options cannot use spread",
                ));
            };
            let Prop::KeyValue(property) = &**property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.slider options must use key-value pairs",
                ));
            };
            let PropName::Ident(name) = &property.key else {
                return Err(self.error(
                    property.key.span(),
                    "MTS012",
                    "ui.slider option names must be identifiers",
                ));
            };
            match name.sym.as_ref() {
                "onChange" if !saw_change => {
                    saw_change = true;
                    let arrow = as_arrow(&property.value).ok_or_else(|| {
                        self.error(
                            property.value.span(),
                            "MTS012",
                            "onChange arrow is required",
                        )
                    })?;
                    self.nodes[node.0 as usize].on_click =
                        Some(self.add_input_function(arrow, ScalarType::Number)?);
                }
                "onChange" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        format!("duplicate ui.slider property `{}`", name.sym),
                    ));
                }
                "min" if min_value.is_none() => {
                    min_value = Some(self.numeric_option(property.value.span(), &property.value)?);
                }
                "min" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        "duplicate ui.slider property `min`",
                    ));
                }
                "max" if max_value.is_none() => {
                    max_value = Some(self.numeric_option(property.value.span(), &property.value)?);
                }
                "max" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        "duplicate ui.slider property `max`",
                    ));
                }
                _ => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS002",
                        format!("unknown ui.slider property `{}`", name.sym),
                    ));
                }
            }
        }
        if let (Some(min), Some(max)) = (min_value, max_value) {
            self.nodes[node.0 as usize].range = Some((min, max));
        }
        Ok(())
    }

    fn lower_place_layout(&mut self, expression: &Expr) -> Result<LayoutSpec, Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.place layout must be an object",
            ));
        };
        let mut align_name: Option<(String, Span)> = None;
        let mut left = None;
        let mut top = None;
        let mut width = None;
        let mut height = None;
        let mut anchor = AnchorSpec::default();
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.place layout cannot use spread",
                ));
            };
            let Prop::KeyValue(property) = &**property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.place layout must use key-value pairs",
                ));
            };
            let PropName::Ident(name) = &property.key else {
                return Err(self.error(
                    property.key.span(),
                    "MTS012",
                    "ui.place layout names must be identifiers",
                ));
            };
            match name.sym.as_ref() {
                "align" => {
                    let Expr::Lit(Lit::Str(value)) = &*property.value else {
                        return Err(self.error(
                            property.value.span(),
                            "MTS012",
                            "ui.place align must be a string",
                        ));
                    };
                    align_name = Some((
                        value.value.as_str().unwrap_or("").to_string(),
                        property.value.span(),
                    ));
                }
                "left" => left = Some(self.numeric_option(property.value.span(), &property.value)?),
                "top" => top = Some(self.numeric_option(property.value.span(), &property.value)?),
                "width" => {
                    width = Some(self.numeric_option(property.value.span(), &property.value)?)
                }
                "height" => {
                    height = Some(self.numeric_option(property.value.span(), &property.value)?)
                }
                "anchor" => {
                    let Expr::Object(anchor_object) = &*property.value else {
                        return Err(self.error(
                            property.value.span(),
                            "MTS012",
                            "ui.place anchor must be an object",
                        ));
                    };
                    for anchor_prop in &anchor_object.props {
                        let PropOrSpread::Prop(anchor_prop) = anchor_prop else {
                            return Err(self.error(
                                anchor_prop.span(),
                                "MTS012",
                                "ui.place anchor cannot use spread",
                            ));
                        };
                        let Prop::KeyValue(anchor_prop) = &**anchor_prop else {
                            return Err(self.error(
                                anchor_prop.span(),
                                "MTS012",
                                "ui.place anchor must use key-value pairs",
                            ));
                        };
                        let PropName::Ident(anchor_name) = &anchor_prop.key else {
                            return Err(self.error(
                                anchor_prop.key.span(),
                                "MTS012",
                                "ui.place anchor names must be identifiers",
                            ));
                        };
                        let value =
                            self.numeric_option(anchor_prop.value.span(), &anchor_prop.value)?;
                        match anchor_name.sym.as_ref() {
                            "left" => anchor.left = Some(value),
                            "top" => anchor.top = Some(value),
                            "right" => anchor.right = Some(value),
                            "bottom" => anchor.bottom = Some(value),
                            _ => {
                                return Err(self.error(
                                    anchor_prop.key.span(),
                                    "MTS002",
                                    format!("unknown ui.place anchor edge `{}`", anchor_name.sym),
                                ));
                            }
                        }
                    }
                }
                _ => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS002",
                        format!("unknown ui.place layout property `{}`", name.sym),
                    ));
                }
            }
        }
        /* `align` seeds a default anchor combo (Delphi Align); explicit anchor
         * edges override per edge (Delphi Anchors). */
        if let Some((name, span)) = align_name {
            match name.as_str() {
                "none" => {}
                "top" => {
                    anchor.left.get_or_insert(0.0);
                    anchor.right.get_or_insert(0.0);
                    anchor.top.get_or_insert(0.0);
                }
                "bottom" => {
                    anchor.left.get_or_insert(0.0);
                    anchor.right.get_or_insert(0.0);
                    anchor.bottom.get_or_insert(0.0);
                }
                "left" => {
                    anchor.left.get_or_insert(0.0);
                    anchor.top.get_or_insert(0.0);
                    anchor.bottom.get_or_insert(0.0);
                }
                "right" => {
                    anchor.right.get_or_insert(0.0);
                    anchor.top.get_or_insert(0.0);
                    anchor.bottom.get_or_insert(0.0);
                }
                "client" => {
                    anchor.left.get_or_insert(0.0);
                    anchor.top.get_or_insert(0.0);
                    anchor.right.get_or_insert(0.0);
                    anchor.bottom.get_or_insert(0.0);
                }
                other => {
                    return Err(self.error(
                        span,
                        "MTS012",
                        format!("unknown ui.place align `{other}`"),
                    ));
                }
            }
        }
        Ok(LayoutSpec {
            left,
            top,
            width,
            height,
            anchor,
        })
    }

    fn lower_string_array(
        &mut self,
        expression: &Expr,
        widget: &str,
    ) -> Result<Vec<u32>, Diagnostic> {
        let Expr::Array(array) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                format!("{widget} expects a string array"),
            ));
        };
        let mut options = Vec::with_capacity(array.elems.len());
        for element in array.elems.iter().flatten() {
            let constant = literal_constant(&element.expr)
                .filter(|constant| matches!(constant, Constant::String(_)))
                .ok_or_else(|| {
                    self.error(
                        element.expr.span(),
                        "MTS012",
                        format!("{widget} options must be string literals"),
                    )
                })?;
            if let Constant::String(value) = &constant {
                self.validate_literal_glyphs(element.expr.span(), value)?;
            }
            options.push(self.intern(constant));
        }
        Ok(options)
    }

    fn lower_dropdown_options(
        &mut self,
        node: NodeId,
        expression: &Expr,
    ) -> Result<(), Diagnostic> {
        self.lower_selection_options(node, expression, "ui.dropdown")
    }

    fn lower_tabview_tabs(
        &mut self,
        expression: &Expr,
    ) -> Result<(Vec<u32>, Vec<NodeId>), Diagnostic> {
        let Expr::Array(array) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.tabview expects a tab array",
            ));
        };
        let mut titles = Vec::with_capacity(array.elems.len());
        let mut contents = Vec::with_capacity(array.elems.len());
        for element in array.elems.iter().flatten() {
            let Expr::Object(object) = &*element.expr else {
                return Err(self.error(
                    element.expr.span(),
                    "MTS012",
                    "tab entries must be object literals",
                ));
            };
            let mut title = None;
            let mut content = None;
            for property in &object.props {
                let PropOrSpread::Prop(property) = property else {
                    return Err(self.error(
                        property.span(),
                        "MTS012",
                        "tab entries cannot use spread",
                    ));
                };
                let Prop::KeyValue(property) = &**property else {
                    return Err(self.error(
                        property.span(),
                        "MTS012",
                        "tab properties must use key-value pairs",
                    ));
                };
                let PropName::Ident(name) = &property.key else {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        "tab property names must be identifiers",
                    ));
                };
                match name.sym.as_ref() {
                    "title" if title.is_none() => {
                        let constant = literal_constant(&property.value)
                            .filter(|constant| matches!(constant, Constant::String(_)))
                            .ok_or_else(|| {
                                self.error(
                                    property.value.span(),
                                    "MTS012",
                                    "tab title must be a string",
                                )
                            })?;
                        if let Constant::String(value) = &constant {
                            self.validate_literal_glyphs(property.value.span(), value)?;
                        }
                        title = Some(self.intern(constant));
                    }
                    "content" if content.is_none() => {
                        content = Some(self.lower_ui(&property.value)?);
                    }
                    "title" | "content" => {
                        return Err(self.error(
                            property.key.span(),
                            "MTS012",
                            format!("duplicate tab property `{}`", name.sym),
                        ));
                    }
                    _ => {
                        return Err(self.error(
                            property.key.span(),
                            "MTS002",
                            format!("unknown tab property `{}`", name.sym),
                        ));
                    }
                }
            }
            titles.push(title.ok_or_else(|| {
                self.error(element.expr.span(), "MTS012", "tab is missing a title")
            })?);
            contents.push(content.ok_or_else(|| {
                self.error(element.expr.span(), "MTS012", "tab is missing content")
            })?);
        }
        Ok((titles, contents))
    }

    fn lower_list_items(&mut self, expression: &Expr) -> Result<Vec<NodeId>, Diagnostic> {
        let Expr::Array(array) = expression else {
            return Err(self.error(expression.span(), "MTS012", "ui.list expects an item array"));
        };
        let mut children = Vec::with_capacity(array.elems.len());
        for element in array.elems.iter().flatten() {
            let Expr::Object(object) = &*element.expr else {
                return Err(self.error(
                    element.expr.span(),
                    "MTS012",
                    "list items must be object literals",
                ));
            };
            let mut text = None;
            let mut handler = None;
            for property in &object.props {
                let PropOrSpread::Prop(property) = property else {
                    return Err(self.error(
                        property.span(),
                        "MTS012",
                        "list items cannot use spread",
                    ));
                };
                let Prop::KeyValue(property) = &**property else {
                    return Err(self.error(
                        property.span(),
                        "MTS012",
                        "list item properties must use key-value pairs",
                    ));
                };
                let PropName::Ident(name) = &property.key else {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        "list item property names must be identifiers",
                    ));
                };
                match name.sym.as_ref() {
                    "text" if text.is_none() => {
                        let constant = literal_constant(&property.value)
                            .filter(|constant| matches!(constant, Constant::String(_)))
                            .ok_or_else(|| {
                                self.error(
                                    property.value.span(),
                                    "MTS012",
                                    "list item text must be a string",
                                )
                            })?;
                        if let Constant::String(value) = &constant {
                            self.validate_literal_glyphs(property.value.span(), value)?;
                        }
                        text = Some(self.intern(constant));
                    }
                    "onClick" if handler.is_none() => {
                        let arrow = as_arrow(&property.value).ok_or_else(|| {
                            self.error(
                                property.value.span(),
                                "MTS012",
                                "list item onClick must be an arrow",
                            )
                        })?;
                        handler = Some(self.add_function(arrow, false)?);
                    }
                    "text" | "onClick" => {
                        return Err(self.error(
                            property.key.span(),
                            "MTS012",
                            format!("duplicate list item property `{}`", name.sym),
                        ));
                    }
                    _ => {
                        return Err(self.error(
                            property.key.span(),
                            "MTS002",
                            format!("unknown list item property `{}`", name.sym),
                        ));
                    }
                }
            }
            let row = self.reserve_node(UiKind::Button);
            if let Some(text) = text {
                self.nodes[row.0 as usize].text = Some(TextSource::Constant(text));
            }
            if let Some(handler) = handler {
                self.nodes[row.0 as usize].on_click = Some(handler);
            }
            children.push(row);
        }
        Ok(children)
    }

    fn lower_scale_options(&mut self, node: NodeId, expression: &Expr) -> Result<(), Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.scale options must be an object",
            ));
        };
        let mut min_value = None;
        let mut max_value = None;
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.scale options cannot use spread",
                ));
            };
            let Prop::KeyValue(property) = &**property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.scale options must use key-value pairs",
                ));
            };
            let PropName::Ident(name) = &property.key else {
                return Err(self.error(
                    property.key.span(),
                    "MTS012",
                    "ui.scale option names must be identifiers",
                ));
            };
            match name.sym.as_ref() {
                "min" if min_value.is_none() => {
                    min_value = Some(self.numeric_option(property.value.span(), &property.value)?);
                }
                "max" if max_value.is_none() => {
                    max_value = Some(self.numeric_option(property.value.span(), &property.value)?);
                }
                "min" | "max" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        format!("duplicate ui.scale property `{}`", name.sym),
                    ));
                }
                _ => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS002",
                        format!("unknown ui.scale property `{}`", name.sym),
                    ));
                }
            }
        }
        if let (Some(min), Some(max)) = (min_value, max_value) {
            self.nodes[node.0 as usize].range = Some((min, max));
        }
        Ok(())
    }

    fn lower_selection_options(
        &mut self,
        node: NodeId,
        expression: &Expr,
        widget: &str,
    ) -> Result<(), Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                format!("{widget} options must be an object"),
            ));
        };
        let mut saw_change = false;
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    format!("{widget} options cannot use spread"),
                ));
            };
            let Prop::KeyValue(property) = &**property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    format!("{widget} options must use key-value pairs"),
                ));
            };
            let PropName::Ident(name) = &property.key else {
                return Err(self.error(
                    property.key.span(),
                    "MTS012",
                    format!("{widget} option names must be identifiers"),
                ));
            };
            match name.sym.as_ref() {
                "onChange" if !saw_change => {
                    saw_change = true;
                    let arrow = as_arrow(&property.value).ok_or_else(|| {
                        self.error(
                            property.value.span(),
                            "MTS012",
                            "onChange arrow is required",
                        )
                    })?;
                    self.nodes[node.0 as usize].on_click =
                        Some(self.add_input_function(arrow, ScalarType::Number)?);
                }
                "onChange" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        format!("duplicate {widget} property `{}`", name.sym),
                    ));
                }
                _ => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS002",
                        format!("unknown {widget} property `{}`", name.sym),
                    ));
                }
            }
        }
        Ok(())
    }

    fn lower_checkbox_options(
        &mut self,
        node: NodeId,
        expression: &Expr,
    ) -> Result<(), Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.checkbox options must be an object",
            ));
        };
        let mut saw_change = false;
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.checkbox options cannot use spread",
                ));
            };
            let Prop::KeyValue(property) = &**property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.checkbox options must use key-value pairs",
                ));
            };
            let PropName::Ident(name) = &property.key else {
                return Err(self.error(
                    property.key.span(),
                    "MTS012",
                    "ui.checkbox option names must be identifiers",
                ));
            };
            match name.sym.as_ref() {
                "onChange" if !saw_change => {
                    saw_change = true;
                    let arrow = as_arrow(&property.value).ok_or_else(|| {
                        self.error(
                            property.value.span(),
                            "MTS012",
                            "onChange arrow is required",
                        )
                    })?;
                    self.nodes[node.0 as usize].on_click =
                        Some(self.add_input_function(arrow, ScalarType::Bool)?);
                }
                "onChange" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        format!("duplicate ui.checkbox property `{}`", name.sym),
                    ));
                }
                _ => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS002",
                        format!("unknown ui.checkbox property `{}`", name.sym),
                    ));
                }
            }
        }
        Ok(())
    }

    fn numeric_option(&mut self, span: Span, expression: &Expr) -> Result<f64, Diagnostic> {
        let constant = literal_constant(expression)
            .ok_or_else(|| self.error(span, "MTS012", "option value must be a number"))?;
        let Constant::Number(value) = constant else {
            return Err(self.error(span, "MTS012", "option value must be a number"));
        };
        Ok(value)
    }

    fn lower_text_style(&self, expression: &Expr) -> Result<TextStyle, Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS014",
                "text style must be an object literal",
            ));
        };
        let mut fields = BTreeMap::new();
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                return Err(self.error(property.span(), "MTS014", "text style cannot use spread"));
            };
            let Prop::KeyValue(property) = &**property else {
                return Err(self.error(
                    property.span(),
                    "MTS014",
                    "text style fields must be literal key-value pairs",
                ));
            };
            let PropName::Ident(name) = &property.key else {
                return Err(self.error(
                    property.key.span(),
                    "MTS014",
                    "text style field names must be identifiers",
                ));
            };
            let name = name.sym.to_string();
            if !matches!(name.as_str(), "font" | "size" | "weight" | "lineHeight") {
                return Err(self.error(
                    property.key.span(),
                    "MTS014",
                    format!("unknown text style field `{name}`"),
                ));
            }
            if fields.insert(name.clone(), &*property.value).is_some() {
                return Err(self.error(
                    property.key.span(),
                    "MTS014",
                    format!("duplicate text style field `{name}`"),
                ));
            }
        }

        for required in ["font", "size", "weight", "lineHeight"] {
            if !fields.contains_key(required) {
                return Err(self.error(
                    object.span,
                    "MTS014",
                    format!("text style field `{required}` is required"),
                ));
            }
        }

        let font = string_literal(fields["font"]).ok_or_else(|| {
            self.error(
                fields["font"].span(),
                "MTS014",
                "text style `font` must be a string literal",
            )
        })?;
        let family = match font {
            "uiSans" => FontFamily::UiSans,
            _ => {
                return Err(self.error(
                    fields["font"].span(),
                    "MTS014",
                    format!("unknown font `{font}`"),
                ));
            }
        };
        let weight = string_literal(fields["weight"]).ok_or_else(|| {
            self.error(
                fields["weight"].span(),
                "MTS014",
                "text style `weight` must be a string literal",
            )
        })?;
        let weight = match weight {
            "regular" => FontWeight::Regular,
            "medium" | "bold" => {
                return Err(self.error(
                    fields["weight"].span(),
                    "MTS014",
                    format!("font weight `{weight}` has no generated uiSans asset; use `regular`"),
                ));
            }
            _ => {
                return Err(self.error(
                    fields["weight"].span(),
                    "MTS014",
                    format!("unknown font weight `{weight}`"),
                ));
            }
        };
        let size_px = u8_literal(fields["size"]).ok_or_else(|| {
            self.error(
                fields["size"].span(),
                "MTS014",
                "text style `size` must be an unsigned 8-bit integer literal",
            )
        })?;
        let line_height_px = u8_literal(fields["lineHeight"]).ok_or_else(|| {
            self.error(
                fields["lineHeight"].span(),
                "MTS014",
                "text style `lineHeight` must be an unsigned 8-bit integer literal",
            )
        })?;

        TextStyle::new(family, size_px, weight, line_height_px)
            .map_err(|error| self.error(expression.span(), "MTS014", error.to_string()))
    }

    /// Lower `ui.surface(tone, { onClick, label })`. `label` is the accessible
    /// name for a tappable surface; it must be a literal so a re-render can
    /// never replace the surface's painted content.
    fn lower_surface_options<'b>(
        &self,
        expression: &'b Expr,
    ) -> Result<(Option<&'b ArrowExpr>, Option<String>), Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.surface options must be an object",
            ));
        };
        let mut handler = None;
        let mut label = None;
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.surface options cannot use spread",
                ));
            };
            let Prop::KeyValue(property) = &**property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.surface options must use key-value pairs",
                ));
            };
            let PropName::Ident(name) = &property.key else {
                return Err(self.error(
                    property.key.span(),
                    "MTS012",
                    "ui.surface option names must be identifiers",
                ));
            };
            match name.sym.as_ref() {
                "onClick" if handler.is_none() => {
                    handler = Some(as_arrow(&property.value).ok_or_else(|| {
                        self.error(property.value.span(), "MTS012", "onClick arrow is required")
                    })?);
                }
                "label" if label.is_none() => {
                    let constant = literal_constant(&property.value).ok_or_else(|| {
                        self.error(
                            property.value.span(),
                            "MTS012",
                            "ui.surface label must be a string literal",
                        )
                    })?;
                    let Constant::String(value) = constant else {
                        return Err(self.error(
                            property.value.span(),
                            "MTS012",
                            "ui.surface label must be a string literal",
                        ));
                    };
                    self.validate_literal_glyphs(property.value.span(), &value)?;
                    label = Some(value);
                }
                "onClick" | "label" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        format!("duplicate ui.surface property `{}`", name.sym),
                    ));
                }
                _ => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS002",
                        format!("unknown ui.surface property `{}`", name.sym),
                    ));
                }
            }
        }
        Ok((handler, label))
    }

    /// Lower `ui.icon(name, { onClick, textStyle, tone, variant })`.
    ///
    /// The tone and variant are interned as two consecutive `options` entries
    /// (`ink|muted|accent|inverse`, then `plain|badge`) so every renderer can
    /// read them by position. `textStyle.size_px` picks the glyph size.
    fn lower_icon_options<'b>(
        &self,
        expression: Option<&'b Expr>,
    ) -> Result<IconOptions<'b>, Diagnostic> {
        let mut handler = None;
        let mut text_style = None;
        let mut tone = None;
        let mut variant = None;
        if let Some(expression) = expression {
            let Expr::Object(object) = expression else {
                return Err(self.error(
                    expression.span(),
                    "MTS012",
                    "ui.icon options must be an object",
                ));
            };
            for property in &object.props {
                let PropOrSpread::Prop(property) = property else {
                    return Err(self.error(
                        property.span(),
                        "MTS012",
                        "ui.icon options cannot use spread",
                    ));
                };
                let Prop::KeyValue(property) = &**property else {
                    return Err(self.error(
                        property.span(),
                        "MTS012",
                        "ui.icon options must use key-value pairs",
                    ));
                };
                let PropName::Ident(name) = &property.key else {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        "ui.icon option names must be identifiers",
                    ));
                };
                match name.sym.as_ref() {
                    "onClick" if handler.is_none() => {
                        handler = Some(as_arrow(&property.value).ok_or_else(|| {
                            self.error(property.value.span(), "MTS012", "onClick arrow is required")
                        })?);
                    }
                    "textStyle" if text_style.is_none() => {
                        text_style = Some(self.lower_text_style(&property.value)?);
                    }
                    "tone" if tone.is_none() => {
                        tone = Some(self.lower_icon_option_value(
                            &property.value,
                            "tone",
                            &["ink", "muted", "accent", "inverse"],
                        )?);
                    }
                    "variant" if variant.is_none() => {
                        variant = Some(self.lower_icon_option_value(
                            &property.value,
                            "variant",
                            &["plain", "badge"],
                        )?);
                    }
                    "onClick" | "textStyle" | "tone" | "variant" => {
                        return Err(self.error(
                            property.key.span(),
                            "MTS012",
                            format!("duplicate ui.icon property `{}`", name.sym),
                        ));
                    }
                    _ => {
                        return Err(self.error(
                            property.key.span(),
                            "MTS002",
                            format!("unknown ui.icon property `{}`", name.sym),
                        ));
                    }
                }
            }
        }
        Ok((
            handler,
            text_style,
            tone.unwrap_or_else(|| "ink".to_owned()),
            variant.unwrap_or_else(|| "plain".to_owned()),
        ))
    }

    fn lower_icon_option_value(
        &self,
        expression: &Expr,
        name: &str,
        allowed: &[&str],
    ) -> Result<String, Diagnostic> {
        let constant = literal_constant(expression).ok_or_else(|| {
            self.error(
                expression.span(),
                "MTS012",
                format!("ui.icon {name} must be a string literal"),
            )
        })?;
        let Constant::String(value) = constant else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                format!("ui.icon {name} must be a string literal"),
            ));
        };
        if !allowed.contains(&value.as_str()) {
            let options = allowed
                .iter()
                .map(|value| format!("\"{value}\""))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(self.error(
                expression.span(),
                "MTS012",
                format!("ui.icon {name} must be one of {options}"),
            ));
        }
        Ok(value)
    }

    fn lower_button_options<'b>(
        &self,
        expression: &'b Expr,
    ) -> Result<(&'b ArrowExpr, Option<TextStyle>, Option<String>), Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.button options must be an object",
            ));
        };
        let mut handler = None;
        let mut text_style = None;
        let mut variant = None;
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.button options cannot use spread",
                ));
            };
            let Prop::KeyValue(property) = &**property else {
                return Err(self.error(
                    property.span(),
                    "MTS012",
                    "ui.button options must use key-value pairs",
                ));
            };
            let PropName::Ident(name) = &property.key else {
                return Err(self.error(
                    property.key.span(),
                    "MTS012",
                    "ui.button option names must be identifiers",
                ));
            };
            match name.sym.as_ref() {
                "onClick" if handler.is_none() => {
                    handler = Some(as_arrow(&property.value).ok_or_else(|| {
                        self.error(property.value.span(), "MTS012", "onClick arrow is required")
                    })?);
                }
                "textStyle" if text_style.is_none() => {
                    text_style = Some(self.lower_text_style(&property.value)?);
                }
                "variant" if variant.is_none() => {
                    let constant = literal_constant(&property.value).ok_or_else(|| {
                        self.error(
                            property.value.span(),
                            "MTS012",
                            "ui.button variant must be a string literal",
                        )
                    })?;
                    let Constant::String(value) = constant else {
                        return Err(self.error(
                            property.value.span(),
                            "MTS012",
                            "ui.button variant must be a string literal",
                        ));
                    };
                    if value != "default" && value != "selected" {
                        return Err(self.error(
                            property.value.span(),
                            "MTS012",
                            "ui.button variant must be \"default\" or \"selected\"",
                        ));
                    }
                    variant = Some(value);
                }
                "onClick" | "textStyle" | "variant" => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS012",
                        format!("duplicate ui.button property `{}`", name.sym),
                    ));
                }
                _ => {
                    return Err(self.error(
                        property.key.span(),
                        "MTS002",
                        format!("unknown ui.button property `{}`", name.sym),
                    ));
                }
            }
        }
        let handler = handler
            .ok_or_else(|| self.error(expression.span(), "MTS012", "onClick arrow is required"))?;
        Ok((handler, text_style, variant))
    }

    fn validate_literal_glyphs(&self, span: Span, value: &str) -> Result<(), Diagnostic> {
        if let Some(glyph) = value.chars().find(|glyph| !is_bootstrap_glyph(*glyph)) {
            return Err(self.error(
                span,
                "MTS015",
                format!(
                    "text literal contains glyph `{glyph}` missing from the bootstrap font manifest"
                ),
            ));
        }
        Ok(())
    }

    /// A literal icon name is either a shared OS icon (`micro_ir::icons`) or
    /// a single display glyph for an App's own letter mark.
    fn validate_icon_name(&self, span: Span, name: &str) -> Result<(), Diagnostic> {
        if micro_ir::icons::is_icon_name(name) {
            return Ok(());
        }
        if name.chars().count() == 1 {
            return self.validate_literal_glyphs(span, name);
        }
        Err(self.error(
            span,
            "MTS012",
            format!(
                "unknown icon `{name}`; use a shared OS icon name (e.g. \"settings\") or a single glyph"
            ),
        ))
    }

    fn add_function(&mut self, arrow: &ArrowExpr, binding: bool) -> Result<FunctionId, Diagnostic> {
        let kind = if binding {
            let id = BindingId(self.binding_count);
            self.binding_count += 1;
            FunctionKind::Binding(id)
        } else {
            let id = HandlerId(self.handler_count);
            self.handler_count += 1;
            FunctionKind::Handler(id)
        };
        let function = FunctionLowerer::new(self, kind).lower_arrow(arrow)?;
        let id = FunctionId(self.functions.len() as u32);
        self.functions.push(function);
        Ok(id)
    }

    /// Lower a `ui.input` `onChange` handler. The handler accepts exactly one
    /// string argument (the new field text); the lowering binds the arrow's
    /// first parameter name so body reads of it compile to `LoadArg`.
    fn add_input_function(
        &mut self,
        arrow: &ArrowExpr,
        arg_type: ScalarType,
    ) -> Result<FunctionId, Diagnostic> {
        if arrow.params.len() != 1 {
            return Err(self.error(
                arrow.span,
                "MTS012",
                "onChange handler must take exactly one argument",
            ));
        }
        let Pat::Ident(binding) = &arrow.params[0] else {
            return Err(self.error(
                arrow.params[0].span(),
                "MTS012",
                "onChange argument must be an identifier",
            ));
        };
        let id = HandlerId(self.handler_count);
        self.handler_count += 1;
        let function = FunctionLowerer::new(self, FunctionKind::Handler(id))
            .with_argument(binding.id.sym.to_string(), arg_type)
            .lower_arrow(arrow)?;
        let id = FunctionId(self.functions.len() as u32);
        self.functions.push(function);
        Ok(id)
    }

    fn intern(&mut self, constant: Constant) -> u32 {
        if let Some(index) = self
            .constants
            .iter()
            .position(|existing| existing == &constant)
        {
            index as u32
        } else {
            let index = self.constants.len() as u32;
            self.constants.push(constant);
            index
        }
    }

    fn intern_host_request(&mut self, request: HostRequest) -> u32 {
        if let Some(index) = self
            .host_requests
            .iter()
            .position(|existing| existing == &request)
        {
            index as u32
        } else {
            let index = self.host_requests.len() as u32;
            self.host_requests.push(request);
            index
        }
    }

    fn error(&self, span: Span, code: &'static str, message: impl Into<String>) -> Diagnostic {
        diagnostic_at(self.source_map, self.path, span, code, message.into())
    }
}

struct FunctionLowerer<'lowerer, 'source> {
    parent: &'lowerer mut Lowerer<'source>,
    kind: FunctionKind,
    argument: Option<(String, ScalarType)>,
    locals: BTreeMap<String, (u16, ScalarType)>,
    next_local: u16,
    code: Vec<Instruction>,
}

impl<'lowerer, 'source> FunctionLowerer<'lowerer, 'source> {
    fn new(parent: &'lowerer mut Lowerer<'source>, kind: FunctionKind) -> Self {
        Self {
            parent,
            kind,
            argument: None,
            locals: BTreeMap::new(),
            next_local: 0,
            code: Vec::new(),
        }
    }

    fn with_argument(mut self, name: String, arg_type: ScalarType) -> Self {
        self.argument = Some((name, arg_type));
        self
    }

    fn lower_arrow(mut self, arrow: &ArrowExpr) -> Result<Function, Diagnostic> {
        match (&self.kind, &*arrow.body) {
            (FunctionKind::Binding(_), BlockStmtOrExpr::Expr(expression)) => {
                self.expression(expression)?;
            }
            (FunctionKind::Binding(_), BlockStmtOrExpr::BlockStmt(block)) => {
                for statement in &block.stmts {
                    self.statement(statement)?;
                }
            }
            (_, BlockStmtOrExpr::Expr(expression)) => {
                self.expression(expression)?;
                self.code.push(Instruction::Pop);
            }
            (_, BlockStmtOrExpr::BlockStmt(block)) => {
                for statement in &block.stmts {
                    self.statement(statement)?;
                }
            }
        }
        if !matches!(self.code.last(), Some(Instruction::Return)) {
            self.code.push(Instruction::Return);
        }
        Ok(Function {
            kind: self.kind,
            arg_count: u8::from(self.argument.is_some()),
            locals: self.next_local,
            max_stack: 64,
            code: self.code,
        })
    }

    fn statement(&mut self, statement: &Stmt) -> Result<(), Diagnostic> {
        match statement {
            Stmt::Decl(Decl::Var(declaration)) => {
                for declarator in &declaration.decls {
                    let Pat::Ident(name) = &declarator.name else {
                        unreachable!()
                    };
                    let initializer = declarator.init.as_ref().ok_or_else(|| {
                        self.error(declarator.span, "local initializer is required")
                    })?;
                    let ty = self.expression(initializer)?;
                    let id = self.next_local;
                    self.next_local += 1;
                    self.locals.insert(name.id.sym.to_string(), (id, ty));
                    self.code.push(Instruction::StoreLocal(id));
                }
            }
            Stmt::Expr(statement) => {
                self.expression(&statement.expr)?;
                self.code.push(Instruction::Pop);
            }
            Stmt::Block(block) => {
                for statement in &block.stmts {
                    self.statement(statement)?;
                }
            }
            Stmt::If(statement) => {
                self.expression(&statement.test)?;
                let false_jump = self.emit_jump_if_false();
                self.statement(&statement.cons)?;
                if let Some(alternate) = &statement.alt {
                    let end_jump = self.emit_jump();
                    self.patch(false_jump, self.code.len());
                    self.statement(alternate)?;
                    self.patch(end_jump, self.code.len());
                } else {
                    self.patch(false_jump, self.code.len());
                }
            }
            Stmt::While(statement) => {
                let start = self.code.len();
                self.expression(&statement.test)?;
                let exit = self.emit_jump_if_false();
                self.statement(&statement.body)?;
                self.code.push(Instruction::Jump(start as u32));
                self.patch(exit, self.code.len());
            }
            Stmt::Return(statement) => {
                if let Some(argument) = &statement.arg {
                    self.expression(argument)?;
                }
                self.code.push(Instruction::Return);
            }
            Stmt::Empty(_) => {}
            _ => return Err(self.error(statement.span(), "unsupported function statement")),
        }
        Ok(())
    }

    fn expression(&mut self, expression: &Expr) -> Result<ScalarType, Diagnostic> {
        match expression {
            Expr::Lit(_) => {
                let constant = literal_constant(expression).unwrap();
                let ty = constant.scalar_type();
                let id = self.parent.intern(constant);
                self.code.push(Instruction::Const(id));
                Ok(ty)
            }
            Expr::Ident(identifier) => {
                if self
                    .argument
                    .as_ref()
                    .is_some_and(|(name, _)| name == identifier.sym.as_ref())
                {
                    self.code.push(Instruction::LoadArg);
                    return Ok(self
                        .argument
                        .as_ref()
                        .map_or(ScalarType::String, |(_, ty)| *ty));
                }
                let (id, ty) = self
                    .locals
                    .get(identifier.sym.as_ref())
                    .copied()
                    .ok_or_else(|| self.error(identifier.span, "unknown local"))?;
                self.code.push(Instruction::LoadLocal(id));
                Ok(ty)
            }
            Expr::Member(member) => {
                let (id, ty) = self.state_member(member)?;
                self.code.push(Instruction::LoadState(id));
                Ok(ty)
            }
            Expr::Tpl(template) => {
                let first = template
                    .quasis
                    .first()
                    .map(|quasi| {
                        quasi
                            .cooked
                            .as_ref()
                            .map(|cooked| cooked.to_string_lossy().into_owned())
                            .unwrap_or_else(|| quasi.raw.to_string())
                    })
                    .unwrap_or_default();
                let first_id = self.parent.intern(Constant::String(first));
                self.code.push(Instruction::Const(first_id));
                for (index, expression) in template.exprs.iter().enumerate() {
                    self.expression(expression)?;
                    self.code.push(Instruction::ToString);
                    self.code.push(Instruction::Concat);
                    let tail = template.quasis[index + 1]
                        .cooked
                        .as_ref()
                        .map(|cooked| cooked.to_string_lossy().into_owned())
                        .unwrap_or_else(|| template.quasis[index + 1].raw.to_string());
                    if !tail.is_empty() {
                        let id = self.parent.intern(Constant::String(tail));
                        self.code.push(Instruction::Const(id));
                        self.code.push(Instruction::Concat);
                    }
                }
                Ok(ScalarType::String)
            }
            Expr::Bin(binary) => {
                let left = self.expression(&binary.left)?;
                let right = self.expression(&binary.right)?;
                let (instruction, ty) = match binary.op {
                    BinaryOp::Add => (Instruction::Add, left),
                    BinaryOp::Sub => (Instruction::Sub, ScalarType::Number),
                    BinaryOp::Mul => (Instruction::Mul, ScalarType::Number),
                    BinaryOp::Div => (Instruction::Div, ScalarType::Number),
                    BinaryOp::Mod => (Instruction::Mod, ScalarType::Number),
                    BinaryOp::EqEq | BinaryOp::EqEqEq | BinaryOp::NotEq | BinaryOp::NotEqEq => {
                        (Instruction::Eq, ScalarType::Bool)
                    }
                    /* `a <= b` is `!(a > b)` and `a >= b` is `!(a < b)`;
                     * the `negate` pass below applies the Not. */
                    BinaryOp::Lt => (Instruction::Lt, ScalarType::Bool),
                    BinaryOp::LtEq => (Instruction::Gt, ScalarType::Bool),
                    BinaryOp::Gt => (Instruction::Gt, ScalarType::Bool),
                    BinaryOp::GtEq => (Instruction::Lt, ScalarType::Bool),
                    _ => return Err(self.error(binary.span, "unsupported binary operator")),
                };
                /* `!==` / `!=` is the negation of equality; without the Not the
                 * two operators would lower identically. */
                let negate = matches!(
                    binary.op,
                    BinaryOp::NotEq | BinaryOp::NotEqEq | BinaryOp::LtEq | BinaryOp::GtEq
                );
                if matches!(
                    instruction,
                    Instruction::Add
                        | Instruction::Sub
                        | Instruction::Mul
                        | Instruction::Div
                        | Instruction::Mod
                ) && (left != ScalarType::Number || right != ScalarType::Number)
                {
                    return Err(self.error(binary.span, "arithmetic operands must be numbers"));
                }
                self.code.push(instruction);
                if negate {
                    self.code.push(Instruction::Not);
                }
                Ok(ty)
            }
            Expr::Update(update) => self.update(&update.arg, update.op),
            Expr::Assign(assign) if assign.op == AssignOp::Assign => {
                self.assignment(&assign.left, &assign.right)
            }
            Expr::Paren(parenthesized) => self.expression(&parenthesized.expr),
            Expr::Call(call) => {
                let Some(name) = call_name(call) else {
                    return Err(self.error(expression.span(), "unsupported function call"));
                };
                if !(name.starts_with("device.")
                    || name.starts_with("net.")
                    || name.starts_with("os.")
                    || name.starts_with("clock.")
                    || name.starts_with("kv.")
                    || name.starts_with("config.")
                    || name.starts_with("audio.")
                    || name.starts_with("storage.")
                    || name.starts_with("json."))
                {
                    return Err(self.error(expression.span(), "unsupported function call"));
                }
                self.host_call(&name, call)
            }
            _ => Err(self.error(expression.span(), "unsupported function expression")),
        }
    }

    /// Lower a `device.*` / `net.*` / `storage.*` host call. Calls that return
    /// a value leave it on the stack; void actions leave `Null` so an enclosing
    /// statement's trailing `Pop` stays balanced.
    fn host_call(
        &mut self,
        name: &str,
        call: &swc_ecma_ast::CallExpr,
    ) -> Result<ScalarType, Diagnostic> {
        let Some((kind, arg_kinds, is_async, result_kind)) = host_call_spec(name) else {
            return Err(self.error(call.span, format!("unsupported host call `{name}`")));
        };
        if let Some(capabilities) = &self.parent.metadata.capabilities {
            let capability = kind.capability();
            if !capabilities.contains(&capability) {
                return Err(self.parent.error(
                    call.span,
                    "MTS002",
                    format!("`{name}` requires the `{capability}` capability; add it to app()"),
                ));
            }
        }
        let expected = arg_kinds.len() + usize::from(is_async);
        if call.args.len() != expected {
            return Err(self.error(
                call.span,
                format!("`{name}` expects {expected} argument(s)"),
            ));
        }
        // `playAudio` returns a handle but is still an action (it starts a
        // player), and the storage mutations return a success boolean while
        // still changing the filesystem. Both belong in the same
        // binding-forbidden set as writes. `os.setTitle` is the exception: it
        // only repaints OS chrome and never touches App state, so the launch
        // binding pass may use it to restore the App's title (MBC v28).
        let is_action = (result_kind.is_none() && kind != HostCallKind::OsSetTitle)
            || matches!(
                kind,
                HostCallKind::AudioPlayUrl
                    | HostCallKind::StorageWriteText
                    | HostCallKind::StorageAppendText
                    | HostCallKind::StorageRemove
                    | HostCallKind::StorageMkdir
            );
        if matches!(self.kind, FunctionKind::Binding(_)) && (is_async || is_action) {
            return Err(self.error(
                call.span,
                format!("`{name}` is not allowed inside a binding"),
            ));
        }

        let mut lowered_arg_kinds = Vec::new();
        let mut callback = None;
        for (index, argument) in call.args.iter().enumerate() {
            if is_async && index == call.args.len() - 1 {
                let Expr::Arrow(arrow) = &*argument.expr else {
                    return Err(
                        self.error(argument.span(), "net callback must be an arrow function")
                    );
                };
                callback = Some(self.parent.add_input_function(arrow, ScalarType::String)?);
            } else {
                let ty = self.expression(&argument.expr)?;
                if lowered_arg_kinds.len() < arg_kinds.len()
                    && ty != arg_kinds[lowered_arg_kinds.len()]
                {
                    return Err(self.error(argument.span(), "host call argument type mismatch"));
                }
                lowered_arg_kinds.push(ty);
            }
        }

        let idx = self.parent.intern_host_request(HostRequest {
            kind,
            arg_kinds,
            callback,
            result_kind,
        });
        self.code.push(Instruction::HostCall(idx));
        Ok(match result_kind {
            Some(ty) => ty,
            None => {
                let null = self.parent.intern(Constant::Null);
                self.code.push(Instruction::Const(null));
                ScalarType::Null
            }
        })
    }

    fn update(&mut self, target: &Expr, op: UpdateOp) -> Result<ScalarType, Diagnostic> {
        let one = self.parent.intern(Constant::Number(1.0));
        match target {
            Expr::Member(member) => {
                let (id, ty) = self.state_member(member)?;
                if ty != ScalarType::Number {
                    return Err(self.error(target.span(), "update target must be numeric"));
                }
                self.code.push(Instruction::LoadState(id));
                self.code.push(Instruction::Const(one));
                self.code.push(if op == UpdateOp::PlusPlus {
                    Instruction::Add
                } else {
                    Instruction::Sub
                });
                self.code.push(Instruction::Dup);
                self.code.push(Instruction::StoreState(id));
                Ok(ty)
            }
            Expr::Ident(identifier) => {
                if self
                    .argument
                    .as_ref()
                    .is_some_and(|(name, _)| name == identifier.sym.as_ref())
                {
                    return Err(self.error(target.span(), "cannot update a function argument"));
                }
                let (id, ty) = self
                    .locals
                    .get(identifier.sym.as_ref())
                    .copied()
                    .ok_or_else(|| self.error(identifier.span, "unknown local"))?;
                self.code.push(Instruction::LoadLocal(id));
                self.code.push(Instruction::Const(one));
                self.code.push(if op == UpdateOp::PlusPlus {
                    Instruction::Add
                } else {
                    Instruction::Sub
                });
                self.code.push(Instruction::Dup);
                self.code.push(Instruction::StoreLocal(id));
                Ok(ty)
            }
            _ => Err(self.error(target.span(), "unsupported update target")),
        }
    }

    fn assignment(
        &mut self,
        target: &AssignTarget,
        value: &Expr,
    ) -> Result<ScalarType, Diagnostic> {
        let ty = self.expression(value)?;
        self.code.push(Instruction::Dup);
        match target {
            AssignTarget::Simple(SimpleAssignTarget::Ident(identifier)) => {
                if self
                    .argument
                    .as_ref()
                    .is_some_and(|(name, _)| name == identifier.sym.as_ref())
                {
                    return Err(self.error(identifier.span, "cannot assign to a function argument"));
                }
                let (id, expected) = self
                    .locals
                    .get(identifier.sym.as_ref())
                    .copied()
                    .ok_or_else(|| self.error(identifier.span, "unknown local"))?;
                if ty != expected {
                    return Err(self.error(identifier.span, "assignment type mismatch"));
                }
                self.code.push(Instruction::StoreLocal(id));
            }
            AssignTarget::Simple(SimpleAssignTarget::Member(member)) => {
                let (id, expected) = self.state_member(member)?;
                if ty != expected {
                    return Err(self.error(member.span, "state assignment type mismatch"));
                }
                self.code.push(Instruction::StoreState(id));
            }
            _ => return Err(self.error(target.span(), "unsupported assignment target")),
        }
        Ok(ty)
    }

    fn state_member(&self, member: &MemberExpr) -> Result<(StateId, ScalarType), Diagnostic> {
        let Expr::Ident(object) = &*member.obj else {
            return Err(self.error(member.span, "state access must use an identifier"));
        };
        self.parent
            .state_symbols
            .get(object.sym.as_ref())
            .copied()
            .ok_or_else(|| self.error(member.span, "unknown state"))
    }

    fn emit_jump_if_false(&mut self) -> usize {
        let index = self.code.len();
        self.code.push(Instruction::JumpIfFalse(0));
        index
    }
    fn emit_jump(&mut self) -> usize {
        let index = self.code.len();
        self.code.push(Instruction::Jump(0));
        index
    }
    fn patch(&mut self, index: usize, target: usize) {
        match &mut self.code[index] {
            Instruction::Jump(value) | Instruction::JumpIfFalse(value) => *value = target as u32,
            _ => unreachable!(),
        }
    }
    fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        self.parent.error(span, "MTS013", message)
    }
}

fn literal_constant(expression: &Expr) -> Option<Constant> {
    match expression {
        Expr::Lit(Lit::Num(value)) => Some(Constant::Number(value.value)),
        Expr::Lit(Lit::Str(value)) => {
            Some(Constant::String(value.value.to_string_lossy().into_owned()))
        }
        Expr::Lit(Lit::Bool(value)) => Some(Constant::Bool(value.value)),
        Expr::Lit(Lit::Null(_)) => Some(Constant::Null),
        _ => None,
    }
}

fn string_literal(expression: &Expr) -> Option<&str> {
    let Expr::Lit(Lit::Str(value)) = expression else {
        return None;
    };
    value.value.as_str()
}

fn u8_literal(expression: &Expr) -> Option<u8> {
    let Expr::Lit(Lit::Num(value)) = expression else {
        return None;
    };
    (value.value.fract() == 0.0 && (0.0..=u8::MAX as f64).contains(&value.value))
        .then_some(value.value as u8)
}

fn is_bootstrap_glyph(glyph: char) -> bool {
    glyph == ' '
        || glyph.is_ascii_graphic()
        || include_str!("../../../assets/fonts/ui-sans-common.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .flat_map(str::chars)
            .any(|manifest_glyph| manifest_glyph == glyph)
}

/// The SDK host-call table: name → (kind, argument types, async?, result type).
/// Sync reads return a scalar; actions return nothing; async requests carry a
/// trailing 1-arg callback arrow (not counted in `arg_kinds`).
fn host_call_spec(name: &str) -> Option<(HostCallKind, Vec<ScalarType>, bool, Option<ScalarType>)> {
    Some(match name {
        "device.name" => (
            HostCallKind::DeviceName,
            vec![],
            false,
            Some(ScalarType::String),
        ),
        "device.chip" => (
            HostCallKind::DeviceChip,
            vec![],
            false,
            Some(ScalarType::String),
        ),
        "device.flashBytes" => (
            HostCallKind::DeviceFlashBytes,
            vec![],
            false,
            Some(ScalarType::Number),
        ),
        "device.psramBytes" => (
            HostCallKind::DevicePsramBytes,
            vec![],
            false,
            Some(ScalarType::Number),
        ),
        "device.resetReason" => (
            HostCallKind::DeviceResetReason,
            vec![],
            false,
            Some(ScalarType::String),
        ),
        "device.backlight" => (
            HostCallKind::DeviceBacklight,
            vec![],
            false,
            Some(ScalarType::Number),
        ),
        "device.setBacklight" => (
            HostCallKind::DeviceSetBacklight,
            vec![ScalarType::Number],
            false,
            None,
        ),
        "device.gpioSetup" => (
            HostCallKind::DeviceGpioSetup,
            vec![ScalarType::Number, ScalarType::String],
            false,
            None,
        ),
        "device.gpioWrite" => (
            HostCallKind::DeviceGpioWrite,
            vec![ScalarType::Number, ScalarType::Number],
            false,
            None,
        ),
        "device.gpioRead" => (
            HostCallKind::DeviceGpioRead,
            vec![ScalarType::Number],
            false,
            Some(ScalarType::Number),
        ),
        "net.wifiState" => (
            HostCallKind::NetWifiState,
            vec![],
            false,
            Some(ScalarType::String),
        ),
        "net.wifiSsid" => (
            HostCallKind::NetWifiSsid,
            vec![],
            false,
            Some(ScalarType::String),
        ),
        "net.wifiConnect" => (
            HostCallKind::NetWifiConnect,
            vec![ScalarType::String, ScalarType::String],
            false,
            None,
        ),
        "net.wifiDisconnect" => (HostCallKind::NetWifiDisconnect, vec![], false, None),
        "net.scanWifi" => (HostCallKind::NetScanWifi, vec![], true, None),
        "net.wifiApName" => (
            HostCallKind::NetWifiApName,
            vec![ScalarType::Number],
            false,
            Some(ScalarType::String),
        ),
        "net.httpGet" => (
            HostCallKind::NetHttpGet,
            vec![ScalarType::String],
            true,
            None,
        ),
        "net.httpRequest" => (
            HostCallKind::NetHttpRequest,
            vec![ScalarType::String, ScalarType::String, ScalarType::String],
            true,
            None,
        ),
        "net.httpRequestWithHeaders" => (
            HostCallKind::NetHttpRequestWithHeaders,
            vec![
                ScalarType::String,
                ScalarType::String,
                ScalarType::String,
                ScalarType::String,
            ],
            true,
            None,
        ),
        "net.lookupWords" => (
            HostCallKind::NetLookupWords,
            vec![ScalarType::String],
            true,
            None,
        ),
        "json.getText" => (
            HostCallKind::JsonGetText,
            vec![ScalarType::String, ScalarType::String],
            false,
            Some(ScalarType::String),
        ),
        "json.getNumber" => (
            HostCallKind::JsonGetNumber,
            vec![ScalarType::String, ScalarType::String],
            false,
            Some(ScalarType::Number),
        ),
        "json.length" => (
            HostCallKind::JsonLength,
            vec![ScalarType::String, ScalarType::String],
            false,
            Some(ScalarType::Number),
        ),
        "os.appName" => (
            HostCallKind::OsAppName,
            vec![ScalarType::Number],
            false,
            Some(ScalarType::String),
        ),
        "os.appIcon" => (
            HostCallKind::OsAppIcon,
            vec![ScalarType::Number],
            false,
            Some(ScalarType::String),
        ),
        "os.launchIndex" => (
            HostCallKind::OsLaunchIndex,
            vec![ScalarType::Number],
            false,
            None,
        ),
        "os.goBack" => (HostCallKind::OsGoBack, vec![], false, None),
        "os.delay" => (HostCallKind::OsDelay, vec![ScalarType::Number], true, None),
        "os.setTitle" => (
            HostCallKind::OsSetTitle,
            vec![ScalarType::String],
            false,
            None,
        ),
        "clock.now" => (
            HostCallKind::ClockNow,
            vec![],
            false,
            Some(ScalarType::Number),
        ),
        "kv.getNumber" => (
            HostCallKind::KvGetNumber,
            vec![ScalarType::String],
            false,
            Some(ScalarType::Number),
        ),
        "kv.getText" => (
            HostCallKind::KvGetText,
            vec![ScalarType::String],
            false,
            Some(ScalarType::String),
        ),
        "kv.setNumber" => (
            HostCallKind::KvSetNumber,
            vec![ScalarType::String, ScalarType::Number],
            false,
            None,
        ),
        "kv.setText" => (
            HostCallKind::KvSetText,
            vec![ScalarType::String, ScalarType::String],
            false,
            None,
        ),
        "kv.remove" => (
            HostCallKind::KvRemove,
            vec![ScalarType::String],
            false,
            None,
        ),
        "config.getNumber" => (
            HostCallKind::ConfigGetNumber,
            vec![ScalarType::String],
            false,
            Some(ScalarType::Number),
        ),
        "config.getText" => (
            HostCallKind::ConfigGetText,
            vec![ScalarType::String],
            false,
            Some(ScalarType::String),
        ),
        "config.setNumber" => (
            HostCallKind::ConfigSetNumber,
            vec![ScalarType::String, ScalarType::Number],
            false,
            None,
        ),
        "config.setText" => (
            HostCallKind::ConfigSetText,
            vec![ScalarType::String, ScalarType::String],
            false,
            None,
        ),
        "config.remove" => (
            HostCallKind::ConfigRemove,
            vec![ScalarType::String],
            false,
            None,
        ),
        "audio.speak" => (
            HostCallKind::AudioSpeak,
            vec![ScalarType::String],
            false,
            None,
        ),
        "audio.stop" => (HostCallKind::AudioStop, vec![], false, None),
        "audio.setRate" => (
            HostCallKind::AudioSetRate,
            vec![ScalarType::Number],
            false,
            None,
        ),
        "audio.playAudio" => (
            HostCallKind::AudioPlayUrl,
            vec![ScalarType::String],
            false,
            Some(ScalarType::Number),
        ),
        "audio.stopAudio" => (
            HostCallKind::AudioStopUrl,
            vec![ScalarType::Number],
            false,
            None,
        ),
        "audio.stopAllAudio" => (HostCallKind::AudioStopAllUrls, vec![], false, None),
        "audio.playBackground" => (
            HostCallKind::AudioPlayBackground,
            vec![ScalarType::String],
            false,
            None,
        ),
        "audio.stopBackground" => (HostCallKind::AudioStopBackground, vec![], false, None),
        "storage.available" => (
            HostCallKind::StorageAvailable,
            vec![],
            false,
            Some(ScalarType::Bool),
        ),
        "storage.exists" => (
            HostCallKind::StorageExists,
            vec![ScalarType::String],
            false,
            Some(ScalarType::Bool),
        ),
        "storage.readText" => (
            HostCallKind::StorageReadText,
            vec![ScalarType::String],
            false,
            Some(ScalarType::String),
        ),
        "storage.writeText" => (
            HostCallKind::StorageWriteText,
            vec![ScalarType::String, ScalarType::String],
            false,
            Some(ScalarType::Bool),
        ),
        "storage.appendText" => (
            HostCallKind::StorageAppendText,
            vec![ScalarType::String, ScalarType::String],
            false,
            Some(ScalarType::Bool),
        ),
        "storage.list" => (
            HostCallKind::StorageList,
            vec![ScalarType::String],
            false,
            Some(ScalarType::String),
        ),
        "storage.remove" => (
            HostCallKind::StorageRemove,
            vec![ScalarType::String],
            false,
            Some(ScalarType::Bool),
        ),
        "storage.mkdir" => (
            HostCallKind::StorageMkdir,
            vec![ScalarType::String],
            false,
            Some(ScalarType::Bool),
        ),
        "storage.size" => (
            HostCallKind::StorageSize,
            vec![ScalarType::String],
            false,
            Some(ScalarType::Number),
        ),
        _ => return None,
    })
}

fn call_name_expr(expression: &Expr) -> Option<String> {
    let Expr::Call(call) = expression else {
        return None;
    };
    call_name(call)
}
fn call_name(call: &swc_ecma_ast::CallExpr) -> Option<String> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    match &**callee {
        Expr::Ident(identifier) => Some(identifier.sym.to_string()),
        Expr::Member(member) => {
            let Expr::Ident(object) = &*member.obj else {
                return None;
            };
            let MemberProp::Ident(property) = &member.prop else {
                return None;
            };
            Some(format!("{}.{}", object.sym, property.sym))
        }
        _ => None,
    }
}
fn as_arrow(expression: &Expr) -> Option<&ArrowExpr> {
    match expression {
        Expr::Arrow(arrow) => Some(arrow),
        _ => None,
    }
}
