use std::collections::BTreeMap;

use micro_ir::{
    AppImage, BindingId, Constant, FontFamily, FontWeight, Function, FunctionId, FunctionKind,
    HandlerId, Instruction, NodeId, ScalarType, StateDecl, StateId, TextSource, TextStyle, UiKind,
    UiNodeSpec, ValueSource, validate,
};
use swc_common::{SourceMap, Span, Spanned, sync::Lrc};
use swc_ecma_ast::{
    ArrowExpr, AssignOp, AssignTarget, BinaryOp, BlockStmtOrExpr, Callee, Decl, Expr, Lit,
    MemberExpr, MemberProp, Module, ModuleItem, Pat, Prop, PropName, PropOrSpread,
    SimpleAssignTarget, Stmt, UpdateOp,
};

use crate::{Diagnostic, ParsedProgram, parse::diagnostic_at, parse_validated};

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
        let root = self.lower_ui(mount_argument)?;
        let source_map = self.source_map;
        let path = self.path;
        let image = AppImage {
            constants: self.constants,
            states: self.states,
            functions: self.functions,
            nodes: self.nodes,
            root,
        };
        validate(&image)
            .map_err(|error| diagnostic_at(source_map, path, module.span, "MTS010", error.0))?;
        Ok(image)
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
                if let Some(options) = call.args.get(1) {
                    self.lower_input_options(id, &options.expr)?;
                }
                self.nodes[id.0 as usize].text_style = Some(TextStyle::DEFAULT_TEXT);
                Ok(id)
            }
            Some("ui.button") => {
                let id = self.reserve_node(UiKind::Button);
                let label = literal_constant(&call.args[0].expr)
                    .filter(|constant| matches!(constant, Constant::String(_)))
                    .ok_or_else(|| {
                        self.error(
                            call.args[0].span(),
                            "MTS012",
                            "button label must be a string",
                        )
                    })?;
                if let Constant::String(value) = &label {
                    self.validate_literal_glyphs(call.args[0].span(), value)?;
                }
                self.nodes[id.0 as usize].text = Some(TextSource::Constant(self.intern(label)));
                let (arrow, text_style) = self.lower_button_options(&call.args[1].expr)?;
                self.nodes[id.0 as usize].on_click = Some(self.add_function(arrow, false)?);
                self.nodes[id.0 as usize].text_style =
                    Some(text_style.unwrap_or(TextStyle::DEFAULT_BUTTON));
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

    fn lower_switch_options<'b>(
        &self,
        expression: &'b Expr,
    ) -> Result<&'b ArrowExpr, Diagnostic> {
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
                        self.error(property.value.span(), "MTS012", "onToggle arrow is required")
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
        handler.ok_or_else(|| {
            self.error(expression.span(), "MTS012", "onToggle arrow is required")
        })
    }

    fn lower_input_options(
        &mut self,
        node: NodeId,
        expression: &Expr,
    ) -> Result<(), Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.input options must be an object",
            ));
        };
        let mut saw_change = false;
        let mut saw_placeholder = false;
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
                        self.error(property.value.span(), "MTS012", "onChange arrow is required")
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

    fn lower_slider_options(
        &mut self,
        node: NodeId,
        expression: &Expr,
    ) -> Result<(), Diagnostic> {
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
                        self.error(property.value.span(), "MTS012", "onChange arrow is required")
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
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.list expects an item array",
            ));
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

    fn lower_scale_options(
        &mut self,
        node: NodeId,
        expression: &Expr,
    ) -> Result<(), Diagnostic> {
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
                        self.error(property.value.span(), "MTS012", "onChange arrow is required")
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
                        self.error(property.value.span(), "MTS012", "onChange arrow is required")
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

    fn lower_button_options<'b>(
        &self,
        expression: &'b Expr,
    ) -> Result<(&'b ArrowExpr, Option<TextStyle>), Diagnostic> {
        let Expr::Object(object) = expression else {
            return Err(self.error(
                expression.span(),
                "MTS012",
                "ui.button options must be an object",
            ));
        };
        let mut handler = None;
        let mut text_style = None;
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
                "onClick" | "textStyle" => {
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
        Ok((handler, text_style))
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

    fn error(&self, span: Span, code: &'static str, message: impl Into<String>) -> Diagnostic {
        diagnostic_at(self.source_map, self.path, span, code, message.into())
    }
}

struct FunctionLowerer<'lowerer, 'source> {
    parent: &'lowerer mut Lowerer<'source>,
    kind: FunctionKind,
    argument: Option<(String, ScalarType)>,
    locals: BTreeMap<String, (u16, ScalarType)>,
    code: Vec<Instruction>,
}

impl<'lowerer, 'source> FunctionLowerer<'lowerer, 'source> {
    fn new(parent: &'lowerer mut Lowerer<'source>, kind: FunctionKind) -> Self {
        Self {
            parent,
            kind,
            argument: None,
            locals: BTreeMap::new(),
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
            locals: self.locals.len() as u16,
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
                    let id = self.locals.len() as u16;
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
                    .map(|quasi| quasi.raw.to_string())
                    .unwrap_or_default();
                let first_id = self.parent.intern(Constant::String(first));
                self.code.push(Instruction::Const(first_id));
                for (index, expression) in template.exprs.iter().enumerate() {
                    self.expression(expression)?;
                    self.code.push(Instruction::ToString);
                    self.code.push(Instruction::Concat);
                    let tail = template.quasis[index + 1].raw.to_string();
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
                    BinaryOp::Lt | BinaryOp::LtEq => (Instruction::Lt, ScalarType::Bool),
                    BinaryOp::Gt | BinaryOp::GtEq => (Instruction::Gt, ScalarType::Bool),
                    _ => return Err(self.error(binary.span, "unsupported binary operator")),
                };
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
                Ok(ty)
            }
            Expr::Update(update) => self.update(&update.arg, update.op),
            Expr::Assign(assign) if assign.op == AssignOp::Assign => {
                self.assignment(&assign.left, &assign.right)
            }
            Expr::Paren(parenthesized) => self.expression(&parenthesized.expr),
            _ => Err(self.error(expression.span(), "unsupported function expression")),
        }
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
                    return Err(self.error(
                        target.span(),
                        "cannot update a function argument",
                    ));
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
                    return Err(self.error(
                        identifier.span,
                        "cannot assign to a function argument",
                    ));
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
