//! 実装規則の検査に使うRust構文を抽出する。規則の判定と例外の照合はPython側で行う。

use proc_macro2::{Span, TokenStream, TokenTree};
use quote::ToTokens;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
use syn::{parse::Parser, punctuated::Punctuated, spanned::Spanned, visit::Visit};

fn path(path: &syn::Path) -> Vec<String> {
    path.segments.iter().map(|s| s.ident.to_string()).collect()
}

fn metas(tokens: TokenStream) -> Vec<syn::Meta> {
    Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
        .parse2(tokens)
        .map(|p| p.into_iter().collect())
        .unwrap_or_default()
}

// 条件が不明なOS・feature分岐は両方を検査し、確実にテスト専用の部分だけを除く。
fn production(meta: &syn::Meta) -> Option<bool> {
    match meta {
        syn::Meta::Path(p) if p.is_ident("test") => Some(false),
        syn::Meta::List(list) => {
            let values: Vec<_> = metas(list.tokens.clone()).iter().map(production).collect();
            if list.path.is_ident("not") && values.len() == 1 {
                values[0].map(|v| !v)
            } else if list.path.is_ident("all") {
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.iter().all(|v| *v == Some(true)) {
                    Some(true)
                } else {
                    None
                }
            } else if list.path.is_ident("any") {
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.iter().all(|v| *v == Some(false)) {
                    Some(false)
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .is_ok_and(|m| production(&m) == Some(false))
    })
}

fn doc(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|a| {
            if a.path().is_ident("doc")
                && let syn::Meta::NameValue(n) = &a.meta
                && let syn::Expr::Lit(l) = &n.value
                && let syn::Lit::Str(s) = &l.lit
            {
                return Some(s.value());
            }
            None
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn preceding_comment(source: &str, span: Span) -> String {
    let prefix = &source[..span.byte_range().start.min(source.len())];
    let trimmed = prefix.trim_end();
    if let Some(body) = trimmed.strip_suffix("*/")
        && let Some(start) = body.rfind("/*")
    {
        return body[start + 2..].to_string();
    }
    let mut lines = Vec::new();
    for line in trimmed.lines().rev() {
        let Some(comment) = line.trim_start().strip_prefix("//") else {
            break;
        };
        lines.push(
            comment
                .trim_start_matches('/')
                .trim_start_matches('!')
                .trim(),
        );
    }
    lines.reverse();
    lines.join("\n")
}

fn literal(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Lit(l) => match &l.lit {
            syn::Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        syn::Expr::Paren(p) => literal(&p.expr),
        syn::Expr::Group(g) => literal(&g.expr),
        syn::Expr::Macro(m) if m.mac.path.is_ident("concat") => {
            let args = Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
                .parse2(m.mac.tokens.clone())
                .ok()?;
            args.iter()
                .map(literal)
                .collect::<Option<Vec<_>>>()
                .map(|s| s.concat())
        }
        _ => None,
    }
}

#[derive(Default)]
struct Strings(Vec<String>);
impl<'ast> Visit<'ast> for Strings {
    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        self.0.push(node.value());
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        for t in node.tokens.clone() {
            match t {
                TokenTree::Literal(l) => {
                    if let Ok(s) =
                        syn::parse2::<syn::LitStr>(TokenStream::from(TokenTree::Literal(l)))
                    {
                        self.0.push(s.value());
                    }
                }
                TokenTree::Group(g) => {
                    let nested = syn::Macro {
                        tokens: g.stream(),
                        ..node.clone()
                    };
                    self.visit_macro(&nested);
                }
                _ => {}
            }
        }
    }
}

struct Scanner<'a> {
    source: &'a str,
    scope: Vec<String>,
    module: Vec<String>,
    function_doc: String,
    comments: Vec<String>,
    refs: Vec<Value>,
    imports: Vec<Value>,
    modules: Vec<Value>,
    diagnostics: Vec<Value>,
    skips: Vec<Value>,
    functions: Vec<Value>,
    ordinals: BTreeMap<String, usize>,
    in_test: bool,
}

impl Scanner<'_> {
    fn reference(&mut self, p: Vec<String>, span: Span) {
        if !self.in_test {
            self.refs.push(json!({"path": p, "scope": self.scope,
            "module": self.module, "line": span.start().line}));
        }
    }

    fn use_tree(&mut self, tree: &syn::UseTree, prefix: Vec<String>) {
        match tree {
            syn::UseTree::Path(p) => {
                let mut next = prefix;
                next.push(p.ident.to_string());
                self.use_tree(&p.tree, next);
            }
            syn::UseTree::Group(g) => {
                for item in &g.items {
                    self.use_tree(item, prefix.clone());
                }
            }
            _ => {
                let (name, alias) = match tree {
                    syn::UseTree::Name(n) => (n.ident.to_string(), n.ident.to_string()),
                    syn::UseTree::Rename(n) => (n.ident.to_string(), n.rename.to_string()),
                    syn::UseTree::Glob(_) => ("*".into(), "*".into()),
                    _ => unreachable!(),
                };
                let mut full = prefix;
                let alias = if name == "self" {
                    full.last().cloned().unwrap_or(alias)
                } else {
                    alias
                };
                if name != "self" {
                    full.push(name);
                }
                self.reference(full.clone(), tree.span());
                if !self.in_test {
                    let import = json!({"path": full, "alias": alias,
                        "scope": self.scope, "module": self.module});
                    if !self.imports.contains(&import) {
                        self.imports.push(import);
                    }
                }
            }
        }
    }

    fn diagnostic(&mut self, kind: &str, args: Vec<syn::Expr>, span: Span) {
        if self.in_test {
            return;
        }
        let rule_index = if kind == "diagnostic!" { 0 } else { 3 };
        let rule = args.get(rule_index).and_then(literal);
        let expression = args
            .get(rule_index)
            .map(|e| e.to_token_stream().to_string())
            .unwrap_or_default();
        let mut strings = Strings::default();
        if let Some(message) = args.get(rule_index + 1) {
            if let Some(s) = literal(message) {
                strings.0.push(s);
            }
            strings.visit_expr(message);
        }
        let key = format!(
            "{}:{kind}:{}",
            self.scope.join("::"),
            rule.as_deref().unwrap_or(&expression)
        );
        let ordinal = self.ordinals.entry(key.clone()).or_default();
        *ordinal += 1;
        self.diagnostics
            .push(json!({"item": format!("{key}#{ordinal}"), "kind": kind,
            "rule": rule, "rule_expression": expression, "line": span.start().line,
            "column": span.start().column + 1, "scope": self.scope, "module": self.module,
            "comment": preceding_comment(self.source, span), "function_doc": self.function_doc,
            "context_comments": self.comments.iter().rev().cloned().collect::<Vec<_>>(),
            "strings": strings.0}));
    }

    fn skip_meta(&mut self, meta: &syn::Meta, attr: &syn::Attribute) {
        if path(meta.path()) == ["rustfmt", "skip"] {
            self.skips.push(json!({"line": attr.span().start().line,
                "comment": preceding_comment(self.source, attr.span())}));
        }
        if let syn::Meta::List(list) = meta
            && list.path.is_ident("cfg_attr")
        {
            for child in metas(list.tokens.clone()).iter().skip(1) {
                self.skip_meta(child, attr);
            }
        }
    }

    fn function(
        &mut self,
        name: String,
        attrs: &[syn::Attribute],
        span: Span,
        body: impl FnOnce(&mut Self),
    ) {
        let old_test = self.in_test;
        self.in_test |= test_only(attrs);
        self.scope.push(name);
        let old_doc = std::mem::replace(&mut self.function_doc, doc(attrs));
        self.functions
            .push(json!({"scope": self.scope, "start": span.start().line, "end": span.end().line}));
        body(self);
        self.function_doc = old_doc;
        self.scope.pop();
        self.in_test = old_test;
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item(&mut self, node: &'ast syn::Item) {
        let attrs = match node {
            syn::Item::Const(n) => &n.attrs,
            syn::Item::Enum(n) => &n.attrs,
            syn::Item::ExternCrate(n) => &n.attrs,
            syn::Item::Fn(n) => &n.attrs,
            syn::Item::ForeignMod(n) => &n.attrs,
            syn::Item::Impl(n) => &n.attrs,
            syn::Item::Macro(n) => &n.attrs,
            syn::Item::Mod(n) => &n.attrs,
            syn::Item::Static(n) => &n.attrs,
            syn::Item::Struct(n) => &n.attrs,
            syn::Item::Trait(n) => &n.attrs,
            syn::Item::TraitAlias(n) => &n.attrs,
            syn::Item::Type(n) => &n.attrs,
            syn::Item::Union(n) => &n.attrs,
            syn::Item::Use(n) => &n.attrs,
            _ => &Vec::new(),
        };
        let old = self.in_test;
        self.in_test |= test_only(attrs);
        syn::visit::visit_item(self, node);
        self.in_test = old;
    }
    fn visit_stmt(&mut self, node: &'ast syn::Stmt) {
        self.comments
            .push(preceding_comment(self.source, node.span()));
        syn::visit::visit_stmt(self, node);
        self.comments.pop();
    }
    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        self.skip_meta(&node.meta, node);
    }
    fn visit_path(&mut self, node: &'ast syn::Path) {
        // 単独の識別子はローカル変数や型の場合がある。import元の依存はuse側で記録する。
        if node.segments.len() > 1 {
            self.reference(path(node), node.span());
        }
        syn::visit::visit_path(self, node);
    }
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        self.use_tree(&node.tree, vec![]);
    }
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let old = self.in_test;
        self.in_test |= test_only(&node.attrs);
        for a in &node.attrs {
            self.visit_attribute(a);
        }
        if let Some((_, items)) = &node.content {
            self.scope.push(node.ident.to_string());
            self.module.push(node.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.scope.pop();
            self.module.pop();
        } else {
            let override_path = node.attrs.iter().find_map(|a| {
                if a.path().is_ident("path")
                    && let syn::Meta::NameValue(n) = &a.meta
                {
                    return literal(&n.value);
                }
                None
            });
            self.modules.push(
                json!({"name": node.ident.to_string(), "module": self.module,
                "path": override_path, "test_only": self.in_test}),
            );
        }
        self.in_test = old;
    }
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.function(node.sig.ident.to_string(), &node.attrs, node.span(), |s| {
            syn::visit::visit_item_fn(s, node)
        });
    }
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.function(node.sig.ident.to_string(), &node.attrs, node.span(), |s| {
            syn::visit::visit_impl_item_fn(s, node)
        });
    }
    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        self.function(node.sig.ident.to_string(), &node.attrs, node.span(), |s| {
            syn::visit::visit_trait_item_fn(s, node)
        });
    }
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        self.scope.push(node.self_ty.to_token_stream().to_string());
        syn::visit::visit_item_impl(self, node);
        self.scope.pop();
    }
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = node.func.as_ref() {
            let names = self.expand_alias(path(&p.path));
            if names.ends_with(&["Diagnostic".into(), "new".into()]) {
                self.diagnostic(
                    "Diagnostic::new",
                    node.args.iter().cloned().collect(),
                    node.span(),
                );
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if self
            .expand_alias(path(&node.path))
            .last()
            .is_some_and(|s| s == "diagnostic")
        {
            let args = Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
                .parse2(node.tokens.clone());
            match args {
                Ok(args) => self.diagnostic("diagnostic!", args.into_iter().collect(), node.span()),
                Err(_) => self.diagnostic("diagnostic!", vec![], node.span()),
            }
        }
        self.reference(path(&node.path), node.span());
        // マクロ内にも依存パスがあるため、文字列を除いてトークンを走査する。
        self.macro_tokens(node.tokens.clone());
    }
}

impl Scanner<'_> {
    fn expand_alias(&self, mut names: Vec<String>) -> Vec<String> {
        for _ in 0..20 {
            let Some(first) = names.first() else { break };
            let import = self
                .imports
                .iter()
                .filter(|item| {
                    item["alias"].as_str() == Some(first.as_str())
                        && item["scope"].as_array().is_some_and(|s| {
                            s.len() <= self.scope.len()
                                && s.iter()
                                    .zip(&self.scope)
                                    .all(|(a, b)| a.as_str() == Some(b.as_str()))
                        })
                })
                .max_by_key(|item| item["scope"].as_array().unwrap().len());
            let Some(import) = import else { break };
            let replacement: Vec<_> = import["path"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p.as_str().unwrap().to_string())
                .chain(names.iter().skip(1).cloned())
                .collect();
            if replacement == names {
                break;
            }
            names = replacement;
        }
        names
    }

    fn macro_tokens(&mut self, tokens: TokenStream) {
        let tokens: Vec<_> = tokens.into_iter().collect();
        let mut i = 0;
        while i < tokens.len() {
            if let TokenTree::Group(g) = &tokens[i] {
                self.macro_tokens(g.stream());
            }
            if let TokenTree::Ident(ident) = &tokens[i] {
                let mut names = vec![ident.to_string()];
                let mut end = i + 1;
                while end + 2 < tokens.len()
                    && tokens[end].to_string() == ":"
                    && tokens[end + 1].to_string() == ":"
                {
                    if let TokenTree::Ident(next) = &tokens[end + 2] {
                        names.push(next.to_string());
                        end += 3;
                    } else {
                        break;
                    }
                }
                let expanded = self.expand_alias(names.clone());
                if let Some(TokenTree::Group(group)) = tokens.get(end + 1)
                    && expanded.last().is_some_and(|n| n == "diagnostic")
                    && tokens.get(end).is_some_and(|t| t.to_string() == "!")
                {
                    let args = Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
                        .parse2(group.stream());
                    self.diagnostic(
                        "diagnostic!",
                        args.map(|a| a.into_iter().collect()).unwrap_or_default(),
                        ident.span(),
                    );
                }
                if let Some(TokenTree::Group(group)) = tokens.get(end)
                    && expanded.ends_with(&["Diagnostic".into(), "new".into()])
                {
                    let args = Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
                        .parse2(group.stream());
                    self.diagnostic(
                        "Diagnostic::new",
                        args.map(|a| a.into_iter().collect()).unwrap_or_default(),
                        ident.span(),
                    );
                }
                if names.len() > 1 {
                    self.reference(names, ident.span());
                }
                i = end;
            } else {
                i += 1;
            }
        }
    }
}

fn scan(source: &str) -> syn::Result<Value> {
    let file = syn::parse_file(source)?;
    let mut scanner = Scanner {
        source,
        scope: vec![],
        module: vec![],
        function_doc: String::new(),
        comments: vec![],
        refs: vec![],
        imports: vec![],
        modules: vec![],
        diagnostics: vec![],
        skips: vec![],
        functions: vec![],
        ordinals: BTreeMap::new(),
        in_test: false,
    };
    scanner.visit_file(&file);
    // useは宣言より前でも有効なので、先に収集した別名を使って診断箇所を抽出し直す。
    scanner.refs.clear();
    scanner.modules.clear();
    scanner.diagnostics.clear();
    scanner.skips.clear();
    scanner.functions.clear();
    scanner.ordinals.clear();
    scanner.visit_file(&file);
    Ok(
        json!({"references": scanner.refs, "imports": scanner.imports, "modules": scanner.modules,
        "diagnostics": scanner.diagnostics, "skips": scanner.skips, "functions": scanner.functions}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let files: BTreeMap<String, String> = serde_json::from_str(&input)?;
    let mut output = BTreeMap::new();
    for (name, source) in files {
        let result = scan(&source).map_err(|e| format!("{name}:{}: {e}", e.span().start().line))?;
        output.insert(name, result);
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

#[cfg(test)]
mod tests;
