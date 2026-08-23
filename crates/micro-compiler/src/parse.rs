use std::collections::BTreeSet;

use swc_common::{FileName, SourceMap, Span, Spanned, sync::Lrc};
use swc_ecma_ast::{
    ArrowExpr, AssignTarget, BlockStmtOrExpr, Callee, Decl, Expr, Lit, MemberExpr, MemberProp,
    Module, ModuleItem, Pat, Prop, PropName, PropOrSpread, SimpleAssignTarget, Stmt, VarDeclKind,
};
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

use crate::Diagnostic;

pub struct ParsedProgram {
    pub module: Module,
    pub source_map: Lrc<SourceMap>,
    pub path: String,
}

pub fn validate_source(path: &str, source: &str) -> Result<(), Vec<Diagnostic>> {
    parse_validated(path, source).map(|_| ())
}

pub fn parse_validated(path: &str, source: &str) -> Result<ParsedProgram, Vec<Diagnostic>> {
    let source_map: Lrc<SourceMap> = Default::default();
    let file =
        source_map.new_source_file(FileName::Custom(path.to_owned()).into(), source.to_owned());
    let lexer = Lexer::new(
        Syntax::Typescript(TsSyntax {
            tsx: false,
            ..Default::default()
        }),
        Default::default(),
        StringInput::from(&*file),
        None,
    );
    let mut parser = Parser::new_from(lexer);
    let module = match parser.parse_module() {
        Ok(module) => module,
        Err(error) => {
            return Err(vec![diagnostic_at(
                &source_map,
                path,
                error.span(),
                "MTS000",
                format!("TypeScript parse error: {:?}", error.kind()),
            )]);
        }
    };
    if let Some(error) = parser.take_errors().into_iter().next() {
        return Err(vec![diagnostic_at(
            &source_map,
            path,
            error.span(),
            "MTS000",
            format!("TypeScript parse error: {:?}", error.kind()),
        )]);
    }

    let mut validator = Validator {
        source_map: &source_map,
        path,
        known: BTreeSet::new(),
        mount_count: 0,
        errors: Vec::new(),
    };
    validator.module(&module);
    if validator.errors.is_empty() {
        Ok(ParsedProgram {
            module,
            source_map,
            path: path.to_owned(),
        })
    } else {
        Err(validator.errors)
    }
}

pub(crate) fn diagnostic_at(
    source_map: &SourceMap,
    path: &str,
    span: Span,
    code: &'static str,
    message: String,
) -> Diagnostic {
    let location = source_map.lookup_char_pos(span.lo());
    Diagnostic {
        code,
        path: path.to_owned(),
        line: location.line,
        column: location.col_display + 1,
        message,
        hint: None,
    }
}

struct Validator<'a> {
    source_map: &'a SourceMap,
    path: &'a str,
    known: BTreeSet<String>,
    mount_count: usize,
    errors: Vec<Diagnostic>,
}

impl Validator<'_> {
    fn module(&mut self, module: &Module) {
        for item in &module.body {
            match item {
                ModuleItem::Stmt(statement) => self.statement(statement),
                ModuleItem::ModuleDecl(declaration) => {
                    self.unsupported(declaration.span(), "runtime module declarations")
                }
            }
        }
    }

    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Decl(Decl::Var(declaration)) => {
                if declaration.kind == VarDeclKind::Var {
                    self.unsupported(declaration.span, "var declarations");
                }
                for declarator in &declaration.decls {
                    let Pat::Ident(binding) = &declarator.name else {
                        self.unsupported(declarator.name.span(), "destructuring");
                        continue;
                    };
                    if let Some(initializer) = &declarator.init {
                        self.expression(initializer);
                    }
                    self.known.insert(binding.id.sym.to_string());
                }
            }
            Stmt::Expr(statement) => self.expression(&statement.expr),
            Stmt::Block(block) => {
                for statement in &block.stmts {
                    self.statement(statement);
                }
            }
            Stmt::If(statement) => {
                self.expression(&statement.test);
                self.statement(&statement.cons);
                if let Some(alternate) = &statement.alt {
                    self.statement(alternate);
                }
            }
            Stmt::While(statement) => {
                self.expression(&statement.test);
                self.statement(&statement.body);
            }
            Stmt::Return(statement) => {
                if let Some(argument) = &statement.arg {
                    self.expression(argument);
                }
            }
            Stmt::Empty(_) => {}
            Stmt::Decl(declaration) => self.unsupported(declaration.span(), "declaration"),
            _ => self.unsupported(statement.span(), "statement"),
        }
    }

    fn expression(&mut self, expression: &Expr) {
        match expression {
            Expr::Lit(Lit::Str(_) | Lit::Bool(_) | Lit::Null(_) | Lit::Num(_)) => {}
            Expr::Ident(identifier) => {
                let name = identifier.sym.as_ref();
                if !self.known.contains(name) && !matches!(name, "state" | "bind" | "ui") {
                    self.unsupported(identifier.span, format!("global `{name}`"));
                }
            }
            Expr::Call(call) => self.call(call),
            Expr::Member(member) => self.member(member),
            Expr::Arrow(arrow) => self.arrow(arrow),
            Expr::Tpl(template) => {
                for expression in &template.exprs {
                    self.expression(expression);
                }
            }
            Expr::Bin(binary) => {
                self.expression(&binary.left);
                self.expression(&binary.right);
            }
            Expr::Unary(unary) => self.expression(&unary.arg),
            Expr::Update(update) => self.assignable(&update.arg),
            Expr::Assign(assign) => {
                self.expression(&assign.right);
                match &assign.left {
                    AssignTarget::Simple(SimpleAssignTarget::Ident(identifier)) => {
                        if !self.known.contains(identifier.sym.as_ref()) {
                            self.unsupported(identifier.span, "assignment target");
                        }
                    }
                    AssignTarget::Simple(SimpleAssignTarget::Member(member)) => self.member(member),
                    _ => self.unsupported(assign.left.span(), "assignment target"),
                }
            }
            Expr::Cond(conditional) => {
                self.expression(&conditional.test);
                self.expression(&conditional.cons);
                self.expression(&conditional.alt);
            }
            Expr::Paren(parenthesized) => self.expression(&parenthesized.expr),
            Expr::Array(array) => {
                for element in &array.elems {
                    match element {
                        Some(element) if element.spread.is_none() => self.expression(&element.expr),
                        Some(element) => self.unsupported(element.span(), "spread"),
                        None => self.unsupported(array.span, "array holes"),
                    }
                }
            }
            _ => self.unsupported(expression.span(), "expression"),
        }
    }

    fn call(&mut self, call: &swc_ecma_ast::CallExpr) {
        let Some(name) = call_name(call) else {
            self.unsupported(call.span, "call target");
            return;
        };
        let expected = match name.as_str() {
            "state" | "bind" | "ui.mount" | "ui.column" | "ui.row" | "ui.progress"
            | "ui.led" | "ui.spinner" | "ui.list" | "ui.tabview" => 1..=1,
            "ui.text" => 1..=2,
            "ui.switch" => 1..=2,
            "ui.input" => 1..=2,
            "ui.slider" => 1..=2,
            "ui.scale" => 1..=2,
            "ui.checkbox" => 2..=3,
            "ui.dropdown" => 2..=3,
            "ui.roller" => 2..=3,
            "ui.button" => 2..=2,
            _ => {
                self.unsupported(call.span, format!("call `{name}`"));
                return;
            }
        };
        if !expected.contains(&call.args.len())
            || call.args.iter().any(|argument| argument.spread.is_some())
        {
            let expected = if expected.start() == expected.end() {
                expected.start().to_string()
            } else {
                format!("{} or {}", expected.start(), expected.end())
            };
            self.sdk_error(
                call.span,
                format!("`{name}` expects {expected} argument(s)"),
            );
            return;
        }
        if name == "ui.mount" {
            self.mount_count += 1;
            if self.mount_count > 1 {
                self.errors.push(diagnostic_at(
                    self.source_map,
                    self.path,
                    call.span,
                    "MTS003",
                    "exactly one ui.mount call is allowed".into(),
                ));
            }
        }
        match name.as_str() {
            "ui.button" => {
                self.expression(&call.args[0].expr);
                self.button_options(&call.args[1].expr);
            }
            "ui.text" => self.expression(&call.args[0].expr),
            "ui.switch" => {
                self.expression(&call.args[0].expr);
                if call.args.len() == 2 {
                    self.switch_options(&call.args[1].expr);
                }
            }
            "ui.input" => {
                self.expression(&call.args[0].expr);
                if call.args.len() == 2 {
                    self.input_options(&call.args[1].expr);
                }
            }
            "ui.slider" => {
                self.expression(&call.args[0].expr);
                if call.args.len() == 2 {
                    self.slider_options(&call.args[1].expr);
                }
            }
            "ui.checkbox" => {
                self.expression(&call.args[0].expr);
                self.expression(&call.args[1].expr);
                if call.args.len() == 3 {
                    self.checkbox_options(&call.args[2].expr);
                }
            }
            "ui.dropdown" => {
                self.expression(&call.args[0].expr);
                self.expression(&call.args[1].expr);
                if call.args.len() == 3 {
                    self.dropdown_options(&call.args[2].expr);
                }
            }
            "ui.roller" => {
                self.expression(&call.args[0].expr);
                self.expression(&call.args[1].expr);
                if call.args.len() == 3 {
                    self.dropdown_options(&call.args[2].expr);
                }
            }
            "ui.scale" => {
                self.expression(&call.args[0].expr);
                if call.args.len() == 2 {
                    self.scale_options(&call.args[1].expr);
                }
            }
            "ui.list" => {
                self.list_items(&call.args[0].expr);
            }
            "ui.tabview" => {
                self.tabview_tabs(&call.args[0].expr);
            }
            _ => {
                for argument in &call.args {
                    self.expression(&argument.expr);
                }
            }
        }
    }

    fn tabview_tabs(&mut self, expression: &Expr) {
        let Expr::Array(array) = expression else {
            self.sdk_error(
                expression.span(),
                "ui.tabview expects a tab array".into(),
            );
            return;
        };
        for element in array.elems.iter().flatten() {
            let Expr::Object(object) = &*element.expr else {
                self.unsupported(element.expr.span(), "tab");
                continue;
            };
            for property in &object.props {
                let PropOrSpread::Prop(property) = property else {
                    self.unsupported(property.span(), "spread");
                    continue;
                };
                let Prop::KeyValue(property) = &**property else {
                    self.unsupported(property.span(), "tab property");
                    continue;
                };
                let PropName::Ident(name) = &property.key else {
                    self.unsupported(property.key.span(), "computed tab property");
                    continue;
                };
                if !matches!(name.sym.as_ref(), "title" | "content") {
                    self.errors.push(diagnostic_at(
                        self.source_map,
                        self.path,
                        name.span,
                        "MTS002",
                        format!("unknown tab property `{}`", name.sym),
                    ));
                }
                if name.sym == *"content" {
                    self.expression(&property.value);
                }
            }
        }
    }

    fn list_items(&mut self, expression: &Expr) {
        let Expr::Array(array) = expression else {
            self.sdk_error(
                expression.span(),
                "ui.list expects an item array".into(),
            );
            return;
        };
        for element in array.elems.iter().flatten() {
            let Expr::Object(object) = &*element.expr else {
                self.unsupported(element.expr.span(), "list item");
                continue;
            };
            for property in &object.props {
                let PropOrSpread::Prop(property) = property else {
                    self.unsupported(property.span(), "spread");
                    continue;
                };
                let Prop::KeyValue(property) = &**property else {
                    self.unsupported(property.span(), "list item property");
                    continue;
                };
                let PropName::Ident(name) = &property.key else {
                    self.unsupported(property.key.span(), "computed list item property");
                    continue;
                };
                if !matches!(name.sym.as_ref(), "text" | "onClick") {
                    self.errors.push(diagnostic_at(
                        self.source_map,
                        self.path,
                        name.span,
                        "MTS002",
                        format!("unknown list item property `{}`", name.sym),
                    ));
                }
                if name.sym == *"onClick" {
                    if let Expr::Arrow(arrow) = &*property.value {
                        self.arrow(arrow);
                    } else {
                        self.expression(&property.value);
                    }
                }
            }
        }
    }

    fn scale_options(&mut self, expression: &Expr) {
        let Expr::Object(object) = expression else {
            self.sdk_error(
                expression.span(),
                "ui.scale options must be an object".into(),
            );
            return;
        };
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                self.unsupported(property.span(), "spread");
                continue;
            };
            let Prop::KeyValue(property) = &**property else {
                self.unsupported(property.span(), "scale property");
                continue;
            };
            let PropName::Ident(name) = &property.key else {
                self.unsupported(property.key.span(), "computed scale property");
                continue;
            };
            if !matches!(name.sym.as_ref(), "min" | "max") {
                self.errors.push(diagnostic_at(
                    self.source_map,
                    self.path,
                    name.span,
                    "MTS002",
                    format!("unknown ui.scale property `{}`", name.sym),
                ));
            }
        }
    }

    fn dropdown_options(&mut self, expression: &Expr) {
        let Expr::Object(object) = expression else {
            self.sdk_error(
                expression.span(),
                "ui.dropdown options must be an object".into(),
            );
            return;
        };
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                self.unsupported(property.span(), "spread");
                continue;
            };
            let Prop::KeyValue(property) = &**property else {
                self.unsupported(property.span(), "dropdown property");
                continue;
            };
            let PropName::Ident(name) = &property.key else {
                self.unsupported(property.key.span(), "computed dropdown property");
                continue;
            };
            if name.sym != *"onChange" {
                self.errors.push(diagnostic_at(
                    self.source_map,
                    self.path,
                    name.span,
                    "MTS002",
                    format!("unknown ui.dropdown property `{}`", name.sym),
                ));
            }
            if name.sym == *"onChange" {
                if let Expr::Arrow(arrow) = &*property.value {
                    self.arrow_with_params(arrow, 1);
                } else {
                    self.expression(&property.value);
                }
            }
        }
    }

    fn checkbox_options(&mut self, expression: &Expr) {
        let Expr::Object(object) = expression else {
            self.sdk_error(
                expression.span(),
                "ui.checkbox options must be an object".into(),
            );
            return;
        };
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                self.unsupported(property.span(), "spread");
                continue;
            };
            let Prop::KeyValue(property) = &**property else {
                self.unsupported(property.span(), "checkbox property");
                continue;
            };
            let PropName::Ident(name) = &property.key else {
                self.unsupported(property.key.span(), "computed checkbox property");
                continue;
            };
            if name.sym != *"onChange" {
                self.errors.push(diagnostic_at(
                    self.source_map,
                    self.path,
                    name.span,
                    "MTS002",
                    format!("unknown ui.checkbox property `{}`", name.sym),
                ));
            }
            if name.sym == *"onChange" {
                if let Expr::Arrow(arrow) = &*property.value {
                    self.arrow_with_params(arrow, 1);
                } else {
                    self.expression(&property.value);
                }
            }
        }
    }

    fn slider_options(&mut self, expression: &Expr) {
        let Expr::Object(object) = expression else {
            self.sdk_error(
                expression.span(),
                "ui.slider options must be an object".into(),
            );
            return;
        };
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                self.unsupported(property.span(), "spread");
                continue;
            };
            let Prop::KeyValue(property) = &**property else {
                self.unsupported(property.span(), "slider property");
                continue;
            };
            let PropName::Ident(name) = &property.key else {
                self.unsupported(property.key.span(), "computed slider property");
                continue;
            };
            if !matches!(name.sym.as_ref(), "onChange" | "min" | "max") {
                self.errors.push(diagnostic_at(
                    self.source_map,
                    self.path,
                    name.span,
                    "MTS002",
                    format!("unknown ui.slider property `{}`", name.sym),
                ));
            }
            if name.sym == *"onChange" {
                if let Expr::Arrow(arrow) = &*property.value {
                    self.arrow_with_params(arrow, 1);
                } else {
                    self.expression(&property.value);
                }
            }
        }
    }

    fn input_options(&mut self, expression: &Expr) {
        let Expr::Object(object) = expression else {
            self.sdk_error(
                expression.span(),
                "ui.input options must be an object".into(),
            );
            return;
        };
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                self.unsupported(property.span(), "spread");
                continue;
            };
            let Prop::KeyValue(property) = &**property else {
                self.unsupported(property.span(), "input property");
                continue;
            };
            let PropName::Ident(name) = &property.key else {
                self.unsupported(property.key.span(), "computed input property");
                continue;
            };
            if !matches!(name.sym.as_ref(), "onChange" | "placeholder") {
                self.errors.push(diagnostic_at(
                    self.source_map,
                    self.path,
                    name.span,
                    "MTS002",
                    format!("unknown ui.input property `{}`", name.sym),
                ));
            }
            if name.sym == *"onChange" {
                if let Expr::Arrow(arrow) = &*property.value {
                    self.arrow_with_params(arrow, 1);
                } else {
                    self.expression(&property.value);
                }
            }
        }
    }

    fn switch_options(&mut self, expression: &Expr) {
        let Expr::Object(object) = expression else {
            self.sdk_error(
                expression.span(),
                "ui.switch options must be an object".into(),
            );
            return;
        };
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                self.unsupported(property.span(), "spread");
                continue;
            };
            let Prop::KeyValue(property) = &**property else {
                self.unsupported(property.span(), "switch property");
                continue;
            };
            let PropName::Ident(name) = &property.key else {
                self.unsupported(property.key.span(), "computed switch property");
                continue;
            };
            if name.sym != *"onToggle" {
                self.errors.push(diagnostic_at(
                    self.source_map,
                    self.path,
                    name.span,
                    "MTS002",
                    format!("unknown ui.switch property `{}`", name.sym),
                ));
            }
            if name.sym == *"onToggle" {
                self.expression(&property.value);
            }
        }
    }

    fn button_options(&mut self, expression: &Expr) {
        let Expr::Object(object) = expression else {
            self.sdk_error(
                expression.span(),
                "ui.button options must be an object".into(),
            );
            return;
        };
        for property in &object.props {
            let PropOrSpread::Prop(property) = property else {
                self.unsupported(property.span(), "spread");
                continue;
            };
            let Prop::KeyValue(property) = &**property else {
                self.unsupported(property.span(), "button property");
                continue;
            };
            let PropName::Ident(name) = &property.key else {
                self.unsupported(property.key.span(), "computed button property");
                continue;
            };
            if !matches!(name.sym.as_ref(), "onClick" | "textStyle") {
                self.errors.push(diagnostic_at(
                    self.source_map,
                    self.path,
                    name.span,
                    "MTS002",
                    format!("unknown ui.button property `{}`", name.sym),
                ));
            }
            if name.sym == *"onClick" {
                self.expression(&property.value);
            }
        }
    }

    fn arrow(&mut self, arrow: &ArrowExpr) {
        self.arrow_with_params(arrow, 0);
    }

    fn arrow_with_params(&mut self, arrow: &ArrowExpr, allowed_params: usize) {
        if arrow.is_async || arrow.is_generator {
            self.unsupported(arrow.span, "async or generator arrow function");
            return;
        }
        if arrow.params.len() > allowed_params {
            self.sdk_error(
                arrow.span,
                format!(
                    "arrow expects at most {allowed_params} parameter(s), found {}",
                    arrow.params.len()
                ),
            );
            return;
        }
        /* The onChange handler of ui.input receives the new text as its single
         * argument. Register the parameter name for the body so references to
         * it are not reported as unknown globals, then restore the set. */
        let snapshot = self.known.clone();
        for param in &arrow.params {
            if let Pat::Ident(binding) = param {
                self.known.insert(binding.id.sym.to_string());
            } else {
                self.unsupported(param.span(), "arrow parameter");
            }
        }
        match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(block) => {
                for statement in &block.stmts {
                    self.statement(statement);
                }
            }
            BlockStmtOrExpr::Expr(expression) => self.expression(expression),
        }
        self.known = snapshot;
    }

    fn member(&mut self, member: &MemberExpr) {
        let MemberProp::Ident(property) = &member.prop else {
            self.unsupported(member.span, "computed property access");
            return;
        };
        if property.sym != *"value" {
            self.unsupported(member.span, format!("property `{}`", property.sym));
        }
        self.expression(&member.obj);
    }

    fn assignable(&mut self, expression: &Expr) {
        match expression {
            Expr::Ident(identifier) if self.known.contains(identifier.sym.as_ref()) => {}
            Expr::Member(member) => self.member(member),
            _ => self.unsupported(expression.span(), "update target"),
        }
    }

    fn unsupported(&mut self, span: Span, construct: impl Into<String>) {
        self.errors.push(diagnostic_at(
            self.source_map,
            self.path,
            span,
            "MTS001",
            format!("unsupported syntax: {}", construct.into()),
        ));
    }

    fn sdk_error(&mut self, span: Span, message: String) {
        self.errors.push(diagnostic_at(
            self.source_map,
            self.path,
            span,
            "MTS002",
            message,
        ));
    }
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
