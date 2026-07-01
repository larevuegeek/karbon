//! Karbon's own template engine (feature `native-templates`).
//!
//! A small Jinja/Twig-style engine — **lexer → parser → AST → render** — with
//! no runtime dependency beyond `serde_json`. Implements [`Renderer`] so it is
//! interchangeable with the Tera / minijinja backends.
//!
//! Supported syntax:
//! - `{{ expr }}` — output, **HTML-auto-escaped** (use `| safe` / `| raw` to opt out)
//! - `{{ a.b.c }}` — path access into the JSON context
//! - `{{ x | filter(arg) }}` — filters (`upper`, `lower`, `length`, `default`,
//!   `escape`, `safe`/`raw`, `capitalize`, `trim`, `join`)
//! - `{% if cond %}…{% elif cond %}…{% else %}…{% endif %}` (truthiness, `==`, `!=`, `<`, `>`)
//! - `{% for item in list %}…{% endfor %}` with `loop.index/index0/first/last`
//! - `{% extends "base.html" %}` + `{% block name %}…{% endblock %}` (inheritance)
//! - `{% include "partial.html" %}`
//! - `{# comment #}`

use std::collections::BTreeMap;
use std::collections::HashMap;

use serde_json::Value;

use super::renderer::Renderer;
use crate::error::{AppError, AppResult};

// ─────────────────────────────────────────────────────────────────
// Engine
// ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Template {
    extends: Option<String>,
    nodes: Vec<Node>,
}

/// Karbon-native template engine. Holds named, pre-parsed templates.
#[derive(Default, Clone)]
pub struct NativeEngine {
    templates: BTreeMap<String, Template>,
}

impl NativeEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load all `*.html`/`*.txt`/`*.xml` templates from a directory (recursively).
    pub fn from_dir(dir: &str) -> AppResult<Self> {
        let mut engine = Self::new();
        let base = std::path::Path::new(dir);
        load_dir(&mut engine, base, base)?;
        Ok(engine)
    }

    /// Register a named template from source.
    pub fn add(&mut self, name: &str, source: &str) -> AppResult<()> {
        let tmpl =
            parse(source).map_err(|e| AppError::Internal(format!("template {name}: {e}")))?;
        self.templates.insert(name.to_string(), tmpl);
        Ok(())
    }

    fn get(&self, name: &str) -> AppResult<&Template> {
        self.templates
            .get(name)
            .ok_or_else(|| AppError::Internal(format!("Template '{name}' not found")))
    }

    fn render_template(&self, name: &str, ctx: &Value) -> AppResult<String> {
        // Resolve the inheritance chain (child → … → root), collecting block
        // overrides (most-derived wins).
        let mut overrides: HashMap<String, Vec<Node>> = HashMap::new();
        let mut current = self.get(name)?;
        loop {
            collect_blocks(&current.nodes, &mut overrides);
            match &current.extends {
                Some(parent) => current = self.get(parent)?,
                None => break,
            }
        }
        let mut out = String::new();
        self.render_nodes(&current.nodes, ctx, &overrides, &mut out)?;
        Ok(out)
    }

    fn render_nodes(
        &self,
        nodes: &[Node],
        ctx: &Value,
        overrides: &HashMap<String, Vec<Node>>,
        out: &mut String,
    ) -> AppResult<()> {
        for node in nodes {
            match node {
                Node::Text(t) => out.push_str(t),
                Node::Output(expr) => out.push_str(&render_output(expr, ctx)),
                Node::If {
                    branches,
                    otherwise,
                } => {
                    let mut matched = false;
                    for (cond, body) in branches {
                        if truthy(&eval(cond, ctx)) {
                            self.render_nodes(body, ctx, overrides, out)?;
                            matched = true;
                            break;
                        }
                    }
                    if !matched && let Some(body) = otherwise {
                        self.render_nodes(body, ctx, overrides, out)?;
                    }
                }
                Node::For { var, iter, body } => {
                    if let Value::Array(arr) = eval(iter, ctx) {
                        let len = arr.len();
                        for (i, item) in arr.iter().enumerate() {
                            let scoped = scope_with_loop(ctx, var, item, i, len);
                            self.render_nodes(body, &scoped, overrides, out)?;
                        }
                    }
                }
                Node::Block { name, body } => {
                    let chosen = overrides.get(name).unwrap_or(body);
                    self.render_nodes(chosen, ctx, overrides, out)?;
                }
                Node::Include(name) => {
                    out.push_str(&self.render_template(name, ctx)?);
                }
            }
        }
        Ok(())
    }
}

fn collect_blocks(nodes: &[Node], out: &mut HashMap<String, Vec<Node>>) {
    for node in nodes {
        match node {
            Node::Block { name, body } => {
                out.entry(name.clone()).or_insert_with(|| body.clone());
                collect_blocks(body, out);
            }
            Node::If {
                branches,
                otherwise,
            } => {
                for (_, body) in branches {
                    collect_blocks(body, out);
                }
                if let Some(body) = otherwise {
                    collect_blocks(body, out);
                }
            }
            Node::For { body, .. } => collect_blocks(body, out),
            _ => {}
        }
    }
}

fn load_dir(
    engine: &mut NativeEngine,
    base: &std::path::Path,
    dir: &std::path::Path,
) -> AppResult<()> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| AppError::Internal(format!("read {}: {e}", dir.display())))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_dir(engine, base, &path)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e, "html" | "txt" | "xml"))
            .unwrap_or(false)
        {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&path)
                .map_err(|e| AppError::Internal(format!("read {rel}: {e}")))?;
            engine.add(&rel, &src)?;
        }
    }
    Ok(())
}

impl Renderer for NativeEngine {
    fn render(&self, name: &str, context: &Value) -> AppResult<String> {
        self.render_template(name, context)
    }

    fn render_str(&self, source: &str, context: &Value) -> AppResult<String> {
        let tmpl = parse(source).map_err(AppError::Internal)?;
        if let Some(parent) = &tmpl.extends {
            // Inline template that extends a registered base: render via overrides.
            let mut overrides: HashMap<String, Vec<Node>> = HashMap::new();
            collect_blocks(&tmpl.nodes, &mut overrides);
            let mut current = self.get(parent)?;
            loop {
                collect_blocks(&current.nodes, &mut overrides);
                match &current.extends {
                    Some(p) => current = self.get(p)?,
                    None => break,
                }
            }
            let mut out = String::new();
            self.render_nodes(&current.nodes, context, &overrides, &mut out)?;
            return Ok(out);
        }
        let mut out = String::new();
        self.render_nodes(&tmpl.nodes, context, &HashMap::new(), &mut out)?;
        Ok(out)
    }

    fn has_template(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }

    fn template_names(&self) -> Vec<String> {
        self.templates.keys().cloned().collect()
    }
}

// ─────────────────────────────────────────────────────────────────
// AST
// ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
enum Node {
    Text(String),
    Output(Expr),
    If {
        branches: Vec<(Expr, Vec<Node>)>,
        otherwise: Option<Vec<Node>>,
    },
    For {
        var: String,
        iter: Expr,
        body: Vec<Node>,
    },
    Block {
        name: String,
        body: Vec<Node>,
    },
    Include(String),
}

#[derive(Clone)]
enum Expr {
    Literal(Value),
    Path(Vec<String>),
    Filter(Box<Expr>, String, Vec<Expr>),
    Compare(Box<Expr>, CmpOp, Box<Expr>),
}

#[derive(Clone, Copy)]
enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

// ─────────────────────────────────────────────────────────────────
// Lexer + parser
// ─────────────────────────────────────────────────────────────────

enum Token {
    Text(String),
    Output(String), // inside {{ }}
    Tag(String),    // inside {% %}
}

fn lex(source: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut text_start = 0;

    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() {
            let open = bytes[i + 1];
            if matches!(open, b'{' | b'%' | b'#') {
                if i > text_start {
                    tokens.push(Token::Text(source[text_start..i].to_string()));
                }
                let (close_a, close_b) = match open {
                    b'{' => (b'}', b'}'),
                    b'%' => (b'%', b'}'),
                    _ => (b'#', b'}'),
                };
                let content_start = i + 2;
                let mut j = content_start;
                while j + 1 < bytes.len() && !(bytes[j] == close_a && bytes[j + 1] == close_b) {
                    j += 1;
                }
                if j + 1 >= bytes.len() {
                    return Err("unterminated tag".to_string());
                }
                let content = source[content_start..j].trim().to_string();
                match open {
                    b'{' => tokens.push(Token::Output(content)),
                    b'%' => tokens.push(Token::Tag(content)),
                    _ => {} // comment: drop
                }
                i = j + 2;
                text_start = i;
                continue;
            }
        }
        i += 1;
    }
    if text_start < bytes.len() {
        tokens.push(Token::Text(source[text_start..].to_string()));
    }
    Ok(tokens)
}

fn parse(source: &str) -> Result<Template, String> {
    let tokens = lex(source)?;
    let mut pos = 0;
    let mut extends = None;

    // Optional leading `{% extends "x" %}` (whitespace-only text before it is ok).
    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Text(t) if t.trim().is_empty() => pos += 1,
            Token::Tag(tag) if tag.split_whitespace().next() == Some("extends") => {
                extends = Some(parse_quoted_arg(tag, "extends")?);
                pos += 1;
                break;
            }
            _ => break,
        }
    }

    let nodes = parse_nodes(&tokens, &mut pos, &[])?;
    if pos != tokens.len() {
        return Err("unexpected end tag".to_string());
    }
    Ok(Template { extends, nodes })
}

fn parse_quoted_arg(tag: &str, keyword: &str) -> Result<String, String> {
    let rest = tag.strip_prefix(keyword).unwrap_or("").trim();
    let rest = rest.trim_matches(|c| c == '"' || c == '\'');
    if rest.is_empty() {
        return Err(format!("{keyword} requires a quoted name"));
    }
    Ok(rest.to_string())
}

fn parse_nodes(tokens: &[Token], pos: &mut usize, stop: &[&str]) -> Result<Vec<Node>, String> {
    let mut nodes = Vec::new();
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Text(t) => {
                nodes.push(Node::Text(t.clone()));
                *pos += 1;
            }
            Token::Output(e) => {
                nodes.push(Node::Output(parse_expr(e)?));
                *pos += 1;
            }
            Token::Tag(tag) => {
                let keyword = tag.split_whitespace().next().unwrap_or("");
                if stop.contains(&keyword) {
                    return Ok(nodes);
                }
                match keyword {
                    "if" => nodes.push(parse_if(tokens, pos)?),
                    "for" => nodes.push(parse_for(tokens, pos)?),
                    "block" => nodes.push(parse_block(tokens, pos)?),
                    "include" => {
                        nodes.push(Node::Include(parse_quoted_arg(tag, "include")?));
                        *pos += 1;
                    }
                    other => return Err(format!("unknown tag '{other}'")),
                }
            }
        }
    }
    Ok(nodes)
}

fn parse_block(tokens: &[Token], pos: &mut usize) -> Result<Node, String> {
    let tag = match &tokens[*pos] {
        Token::Tag(t) => t.clone(),
        _ => return Err("expected block".to_string()),
    };
    let name = tag.strip_prefix("block").unwrap_or("").trim().to_string();
    if name.is_empty() {
        return Err("block requires a name".to_string());
    }
    *pos += 1;
    let body = parse_nodes(tokens, pos, &["endblock"])?;
    expect_tag(tokens, pos, "endblock")?;
    Ok(Node::Block { name, body })
}

fn parse_if(tokens: &[Token], pos: &mut usize) -> Result<Node, String> {
    let mut branches = Vec::new();
    let mut otherwise = None;

    let mut cond_expr = parse_expr(&tag_after_keyword(tokens, *pos))?;
    *pos += 1;
    loop {
        let body = parse_nodes(tokens, pos, &["elif", "else", "endif"])?;
        branches.push((cond_expr.clone(), body));
        let kw = current_tag_keyword(tokens, *pos)?;
        match kw.as_str() {
            "elif" => {
                cond_expr = parse_expr(&tag_after_keyword(tokens, *pos))?;
                *pos += 1;
            }
            "else" => {
                *pos += 1;
                otherwise = Some(parse_nodes(tokens, pos, &["endif"])?);
                expect_tag(tokens, pos, "endif")?;
                break;
            }
            "endif" => {
                *pos += 1;
                break;
            }
            _ => return Err("malformed if".to_string()),
        }
    }
    Ok(Node::If {
        branches,
        otherwise,
    })
}

fn parse_for(tokens: &[Token], pos: &mut usize) -> Result<Node, String> {
    let tag = match &tokens[*pos] {
        Token::Tag(t) => t.clone(),
        _ => return Err("expected for".to_string()),
    };
    let rest = tag.strip_prefix("for").unwrap_or("").trim();
    let (var, iter_src) = rest
        .split_once(" in ")
        .ok_or("malformed for (expected `for x in items`)")?;
    let var = var.trim().to_string();
    let iter = parse_expr(iter_src.trim())?;
    *pos += 1;
    let body = parse_nodes(tokens, pos, &["endfor"])?;
    expect_tag(tokens, pos, "endfor")?;
    Ok(Node::For { var, iter, body })
}

fn current_tag_keyword(tokens: &[Token], pos: usize) -> Result<String, String> {
    match tokens.get(pos) {
        Some(Token::Tag(t)) => Ok(t.split_whitespace().next().unwrap_or("").to_string()),
        _ => Err("expected a {% %} tag".to_string()),
    }
}

fn tag_after_keyword(tokens: &[Token], pos: usize) -> String {
    if let Some(Token::Tag(t)) = tokens.get(pos) {
        let mut parts = t.splitn(2, char::is_whitespace);
        parts.next();
        parts.next().unwrap_or("").trim().to_string()
    } else {
        String::new()
    }
}

fn expect_tag(tokens: &[Token], pos: &mut usize, keyword: &str) -> Result<(), String> {
    if current_tag_keyword(tokens, *pos)? == keyword {
        *pos += 1;
        Ok(())
    } else {
        Err(format!("expected {{% {keyword} %}}"))
    }
}

// ─────────────────────────────────────────────────────────────────
// Expression parser (recursive descent over a char scanner)
// ─────────────────────────────────────────────────────────────────

fn parse_expr(src: &str) -> Result<Expr, String> {
    let mut p = ExprParser {
        chars: src.chars().collect(),
        pos: 0,
    };
    let expr = p.parse_comparison()?;
    p.skip_ws();
    if p.pos != p.chars.len() {
        return Err(format!("trailing characters in expression: {src}"));
    }
    Ok(expr)
}

struct ExprParser {
    chars: Vec<char>,
    pos: usize,
}

impl ExprParser {
    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let left = self.parse_pipeline()?;
        self.skip_ws();
        let op = match (self.peek(), self.chars.get(self.pos + 1).copied()) {
            (Some('='), Some('=')) => Some(CmpOp::Eq),
            (Some('!'), Some('=')) => Some(CmpOp::Ne),
            (Some('<'), Some('=')) => Some(CmpOp::Le),
            (Some('>'), Some('=')) => Some(CmpOp::Ge),
            (Some('<'), _) => Some(CmpOp::Lt),
            (Some('>'), _) => Some(CmpOp::Gt),
            _ => None,
        };
        if let Some(op) = op {
            let advance = matches!(op, CmpOp::Eq | CmpOp::Ne | CmpOp::Le | CmpOp::Ge);
            self.pos += if advance { 2 } else { 1 };
            let right = self.parse_pipeline()?;
            return Ok(Expr::Compare(Box::new(left), op, Box::new(right)));
        }
        Ok(left)
    }

    fn parse_pipeline(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            self.skip_ws();
            if self.peek() == Some('|') {
                self.pos += 1;
                self.skip_ws();
                let name = self.parse_ident()?;
                let mut args = Vec::new();
                self.skip_ws();
                if self.peek() == Some('(') {
                    self.pos += 1;
                    loop {
                        self.skip_ws();
                        if self.peek() == Some(')') {
                            self.pos += 1;
                            break;
                        }
                        args.push(self.parse_primary()?);
                        self.skip_ws();
                        if self.peek() == Some(',') {
                            self.pos += 1;
                        } else if self.peek() == Some(')') {
                            self.pos += 1;
                            break;
                        } else {
                            return Err("expected , or ) in filter args".to_string());
                        }
                    }
                }
                expr = Expr::Filter(Box::new(expr), name, args);
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        match self.peek() {
            Some('"') | Some('\'') => self.parse_string(),
            Some(c) if c.is_ascii_digit() || c == '-' => self.parse_number(),
            Some(c) if c.is_alphabetic() || c == '_' => {
                let ident = self.parse_path()?;
                match ident.first().map(String::as_str) {
                    Some("true") if ident.len() == 1 => Ok(Expr::Literal(Value::Bool(true))),
                    Some("false") if ident.len() == 1 => Ok(Expr::Literal(Value::Bool(false))),
                    _ => Ok(Expr::Path(ident)),
                }
            }
            _ => Err("expected expression".to_string()),
        }
    }

    fn parse_string(&mut self) -> Result<Expr, String> {
        let quote = self.chars[self.pos];
        self.pos += 1;
        let mut s = String::new();
        while self.pos < self.chars.len() && self.chars[self.pos] != quote {
            s.push(self.chars[self.pos]);
            self.pos += 1;
        }
        if self.pos >= self.chars.len() {
            return Err("unterminated string".to_string());
        }
        self.pos += 1; // closing quote
        Ok(Expr::Literal(Value::String(s)))
    }

    fn parse_number(&mut self) -> Result<Expr, String> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while self.pos < self.chars.len()
            && (self.chars[self.pos].is_ascii_digit() || self.chars[self.pos] == '.')
        {
            self.pos += 1;
        }
        let s: String = self.chars[start..self.pos].iter().collect();
        let num: serde_json::Number = s
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .ok_or_else(|| format!("invalid number: {s}"))?;
        Ok(Expr::Literal(Value::Number(num)))
    }

    fn parse_ident(&mut self) -> Result<String, String> {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.chars.len()
            && (self.chars[self.pos].is_alphanumeric() || self.chars[self.pos] == '_')
        {
            self.pos += 1;
        }
        if self.pos == start {
            return Err("expected identifier".to_string());
        }
        Ok(self.chars[start..self.pos].iter().collect())
    }

    fn parse_path(&mut self) -> Result<Vec<String>, String> {
        let mut path = vec![self.parse_ident()?];
        while self.peek() == Some('.') {
            self.pos += 1;
            path.push(self.parse_ident()?);
        }
        Ok(path)
    }
}

// ─────────────────────────────────────────────────────────────────
// Render helpers (expression evaluation)
// ─────────────────────────────────────────────────────────────────

fn scope_with_loop(ctx: &Value, var: &str, item: &Value, index: usize, len: usize) -> Value {
    let mut map = match ctx {
        Value::Object(m) => m.clone(),
        _ => serde_json::Map::new(),
    };
    map.insert(var.to_string(), item.clone());
    map.insert(
        "loop".to_string(),
        serde_json::json!({
            "index": index + 1,
            "index0": index,
            "first": index == 0,
            "last": index + 1 == len,
            "length": len,
        }),
    );
    Value::Object(map)
}

fn render_output(expr: &Expr, ctx: &Value) -> String {
    let (value, safe) = eval_output(expr, ctx);
    let s = value_to_string(&value);
    if safe { s } else { html_escape(&s) }
}

fn eval_output(expr: &Expr, ctx: &Value) -> (Value, bool) {
    match expr {
        Expr::Filter(inner, name, _args) if matches!(name.as_str(), "safe" | "raw") => {
            (eval(inner, ctx), true)
        }
        Expr::Filter(inner, name, args) => {
            let (v, safe) = eval_output(inner, ctx);
            (apply_filter(name, v, args, ctx), safe)
        }
        _ => (eval(expr, ctx), false),
    }
}

fn eval(expr: &Expr, ctx: &Value) -> Value {
    match expr {
        Expr::Literal(v) => v.clone(),
        Expr::Path(path) => lookup(ctx, path),
        Expr::Filter(inner, name, args) => {
            if matches!(name.as_str(), "safe" | "raw") {
                eval(inner, ctx)
            } else {
                apply_filter(name, eval(inner, ctx), args, ctx)
            }
        }
        Expr::Compare(a, op, b) => Value::Bool(compare(&eval(a, ctx), *op, &eval(b, ctx))),
    }
}

fn lookup(ctx: &Value, path: &[String]) -> Value {
    let mut current = ctx;
    for key in path {
        match current {
            Value::Object(m) => match m.get(key) {
                Some(v) => current = v,
                None => return Value::Null,
            },
            _ => return Value::Null,
        }
    }
    current.clone()
}

fn apply_filter(name: &str, value: Value, args: &[Expr], ctx: &Value) -> Value {
    match name {
        "upper" => Value::String(value_to_string(&value).to_uppercase()),
        "lower" => Value::String(value_to_string(&value).to_lowercase()),
        "trim" => Value::String(value_to_string(&value).trim().to_string()),
        "capitalize" => {
            let s = value_to_string(&value);
            let mut chars = s.chars();
            let cap = match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => s,
            };
            Value::String(cap)
        }
        "length" => match &value {
            Value::Array(a) => Value::from(a.len()),
            Value::String(s) => Value::from(s.chars().count()),
            Value::Object(o) => Value::from(o.len()),
            _ => Value::from(0),
        },
        "escape" => Value::String(html_escape(&value_to_string(&value))),
        "default" => {
            if matches!(value, Value::Null) || value_to_string(&value).is_empty() {
                args.first().map(|a| eval(a, ctx)).unwrap_or(Value::Null)
            } else {
                value
            }
        }
        "join" => {
            let sep = args
                .first()
                .map(|a| value_to_string(&eval(a, ctx)))
                .unwrap_or_default();
            if let Value::Array(a) = &value {
                Value::String(a.iter().map(value_to_string).collect::<Vec<_>>().join(&sep))
            } else {
                value
            }
        }
        _ => value, // unknown filter: pass through
    }
}

fn compare(a: &Value, op: CmpOp, b: &Value) -> bool {
    use CmpOp::*;
    match op {
        Eq => a == b,
        Ne => a != b,
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => match op {
                Lt => x < y,
                Gt => x > y,
                Le => x <= y,
                Ge => x >= y,
                _ => false,
            },
            _ => false,
        },
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn render(src: &str, ctx: Value) -> String {
        NativeEngine::new().render_str(src, &ctx).unwrap()
    }

    #[test]
    fn variables_and_paths() {
        assert_eq!(
            render("Hi {{ user.name }}", json!({"user": {"name": "Dave"}})),
            "Hi Dave"
        );
    }

    #[test]
    fn auto_escaping_and_safe() {
        let ctx = json!({"html": "<b>x</b>"});
        assert_eq!(render("{{ html }}", ctx.clone()), "&lt;b&gt;x&lt;/b&gt;");
        assert_eq!(render("{{ html | safe }}", ctx), "<b>x</b>");
    }

    #[test]
    fn filters() {
        assert_eq!(render("{{ name | upper }}", json!({"name": "bob"})), "BOB");
        assert_eq!(render("{{ missing | default('n/a') }}", json!({})), "n/a");
        assert_eq!(
            render("{{ items | length }}", json!({"items": [1, 2, 3]})),
            "3"
        );
        assert_eq!(
            render("{{ items | join(', ') }}", json!({"items": ["a", "b"]})),
            "a, b"
        );
    }

    #[test]
    fn if_elif_else() {
        let tpl = "{% if n > 10 %}big{% elif n > 0 %}small{% else %}zero{% endif %}";
        assert_eq!(render(tpl, json!({"n": 50})), "big");
        assert_eq!(render(tpl, json!({"n": 5})), "small");
        assert_eq!(render(tpl, json!({"n": 0})), "zero");
    }

    #[test]
    fn for_loop_with_loop_vars() {
        let tpl = "{% for x in items %}{{ loop.index }}:{{ x }}{% if loop.last %}.{% endif %}{% endfor %}";
        assert_eq!(render(tpl, json!({"items": ["a", "b"]})), "1:a2:b.");
    }

    #[test]
    fn comments_are_dropped() {
        assert_eq!(render("a{# hidden #}b", json!({})), "ab");
    }

    #[test]
    fn named_templates() {
        let mut engine = NativeEngine::new();
        engine.add("greet.html", "Hello {{ name }}").unwrap();
        assert!(engine.has_template("greet.html"));
        assert_eq!(
            engine.render("greet.html", &json!({"name": "X"})).unwrap(),
            "Hello X"
        );
    }

    #[test]
    fn inheritance_extends_block() {
        let mut engine = NativeEngine::new();
        engine
            .add(
                "base.html",
                "<h1>{% block title %}Default{% endblock %}</h1>[{% block body %}{% endblock %}]",
            )
            .unwrap();
        engine
            .add(
                "page.html",
                "{% extends \"base.html\" %}{% block title %}Home{% endblock %}{% block body %}Hi {{ name }}{% endblock %}",
            )
            .unwrap();
        let out = engine.render("page.html", &json!({"name": "Z"})).unwrap();
        assert_eq!(out, "<h1>Home</h1>[Hi Z]");
    }

    #[test]
    fn block_falls_back_to_parent() {
        let mut engine = NativeEngine::new();
        engine
            .add("base.html", "{% block title %}Default{% endblock %}")
            .unwrap();
        engine
            .add("child.html", "{% extends \"base.html\" %}")
            .unwrap();
        assert_eq!(engine.render("child.html", &json!({})).unwrap(), "Default");
    }

    #[test]
    fn include_partial() {
        let mut engine = NativeEngine::new();
        engine.add("nav.html", "NAV").unwrap();
        engine
            .add("page.html", "top {% include \"nav.html\" %} bottom")
            .unwrap();
        assert_eq!(
            engine.render("page.html", &json!({})).unwrap(),
            "top NAV bottom"
        );
    }
}
