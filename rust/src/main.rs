use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Instant;

mod ast_cache;
mod parse_config;

use oxc_allocator::Allocator;
use oxc_parser::Parser as OxcParser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

use swc_next_allocator::Allocator as SwcNextAllocator;
use swc_next_ecma_ast::{
    Ast as SwcNextAst, ClassType, FunctionType, Lang, NodeKindData, PropertyKind,
    SourceType as SwcNextSourceType, Visit as SwcNextVisit, VisitWith,
};
use swc_next_ecma_parser::{NoTokenParserConfig, Options, Parser as SwcNextParser};
use swc_next_ecma_semantic::{AnalyzeOptions, SyntaxChecks, analyze};

use swc_common::BytePos;
use swc_ecma_parser::{EsSyntax, Parser as SwcParser, StringInput, Syntax, TsSyntax};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const WARMUP: usize = 50;
const RUNS: usize = 300;
struct Stats {
    median: f64,
    min: f64,
    p99: f64,
}

#[derive(Default)]
struct AstShape {
    types: BTreeMap<String, usize>,
    type_paths: BTreeMap<String, String>,
    spans: BTreeMap<(String, u32, u32), usize>,
    span_paths: BTreeMap<String, (String, u32, u32)>,
}

type SyntheticBodySpanMap = BTreeMap<(u32, u32), (u32, u32)>;

fn has_optional_identifier_marker(source: &str, end: u32) -> bool {
    let Some(after_question) = source
        .get(end as usize..)
        .and_then(|suffix| suffix.strip_prefix('?'))
    else {
        return false;
    };
    matches!(
        after_question.trim_start().chars().next(),
        Some(':' | '=' | ',' | ')' | ']' | ';')
    )
}

struct SwcNextAstShape<'a> {
    ast: &'a SwcNextAst<'a>,
    source: &'a str,
    shape: AstShape,
    parents: Vec<(String, bool, Option<u32>)>,
}

impl<'a> SwcNextVisit<'a> for SwcNextAstShape<'a> {
    fn ast(&self) -> &SwcNextAst<'a> {
        self.ast
    }

    fn enter_node(&mut self, node: NodeKindData) {
        let debug_name = format!("{node:?}");
        let id = node.node_id();
        let span = self.ast.span(id);
        let parent_is_method = self.parents.last().is_some_and(|(name, method, _)| {
            name == "MethodDefinition" || (name == "ObjectProperty" && *method)
        });
        let is_method = match &node {
            NodeKindData::ObjectProperty(property) => {
                property.method(self.ast) || property.kind(self.ast) != PropertyKind::Init
            }
            _ => false,
        };
        let parent_type_annotation_end = match &node {
            NodeKindData::FormalParameter(parameter) => parameter
                .type_annotation(self.ast)
                .map(|annotation| self.ast.span(annotation.node_id()).end),
            _ => None,
        };
        let name = match &node {
            NodeKindData::Function(function) => match function.function_type(self.ast) {
                FunctionType::FunctionDeclaration => "FunctionDeclaration".to_owned(),
                FunctionType::FunctionExpression => "FunctionExpression".to_owned(),
            },
            NodeKindData::Class(class) => match class.class_type(self.ast) {
                ClassType::ClassDeclaration => "ClassDeclaration".to_owned(),
                ClassType::ClassExpression => "ClassExpression".to_owned(),
            },
            NodeKindData::BindingProperty(_) => "Property".to_owned(),
            NodeKindData::BindingRestElement(_) | NodeKindData::FormalParameterRest(_) => {
                "RestElement".to_owned()
            }
            NodeKindData::FormalParameter(parameter) => {
                if parameter.initializer(self.ast).is_some() {
                    *self
                        .shape
                        .types
                        .entry("AssignmentPattern".to_owned())
                        .or_default() += 1;
                    *self
                        .shape
                        .spans
                        .entry(("AssignmentPattern".to_owned(), span.start, span.end))
                        .or_default() += 1;
                }
                "FormalParameter".to_owned()
            }
            _ => debug_name
                .split_once('(')
                .map_or(debug_name.clone(), |(name, _)| name.to_owned()),
        };
        let span_end = if name == "ChainExpression"
            && span.end > span.start
            && self.source.get(span.end as usize - 1..span.end as usize) == Some("!")
        {
            span.end - 1
        } else if normalize_type_name(&name) == "Identifier"
            && self
                .parents
                .last()
                .is_some_and(|(parent, _, _)| parent == "FormalParameter")
            && has_optional_identifier_marker(self.source, span.end)
        {
            span.end.saturating_add(1)
        } else if normalize_type_name(&name) == "ObjectPattern"
            && self
                .parents
                .last()
                .is_some_and(|(parent, _, _)| parent == "FormalParameter")
        {
            self.parents
                .last()
                .and_then(|(_, _, annotation_end)| *annotation_end)
                .unwrap_or(span.end)
                .max(span.end)
        } else {
            span.end
        };
        self.parents
            .push((name.clone(), is_method, parent_type_annotation_end));
        if name == "TsThisParameter"
            && self
                .source
                .get(span.start as usize..span.start as usize + 4)
                == Some("this")
        {
            *self.shape.types.entry("Identifier".to_owned()).or_default() += 1;
            *self
                .shape
                .spans
                .entry(("Identifier".to_owned(), span.start, span.start + 4))
                .or_default() += 1;
            return;
        }
        if matches!(
            name.as_str(),
            "FormalParameter" | "FormalParameters" | "TsTupleElement" | "TsIndexSignatureParameter"
        ) {
            return;
        }
        if name == "FunctionExpression" && parent_is_method {
            return;
        }
        let declaration_text = self
            .source
            .get(span.start as usize..span.end as usize)
            .map(str::trim);
        let name = if name == "TsFunction"
            && declaration_text.is_some_and(|text| {
                (text.starts_with("function ") || text.starts_with("declare function "))
                    && text.ends_with(';')
                    && !text.contains('{')
            }) {
            "TSDeclareFunction".to_owned()
        } else {
            normalize_type_name(&name)
        };
        *self.shape.types.entry(name.clone()).or_default() += 1;
        *self
            .shape
            .spans
            .entry((name, span.start, span_end))
            .or_default() += 1;
    }

    fn leave_node(&mut self, _node: NodeKindData) {
        self.parents.pop();
    }
}

fn normalize_type_name(name: &str) -> String {
    match name {
        "Module" | "Script" => "Program".to_owned(),
        "NumericLiteral" | "StringLiteral" | "BooleanLiteral" | "NullLiteral" | "RegExpLiteral"
        | "BigIntLiteral" => "Literal".to_owned(),
        "ObjectProperty" | "KeyValueProperty" | "MethodProperty" | "GetterProperty"
        | "SetterProperty" => "Property".to_owned(),
        "KeyValuePatternProperty" | "AssignmentPatternProperty" => "Property".to_owned(),
        "ClassMethod" | "Constructor" => "MethodDefinition".to_owned(),
        "TsConstAssertion" => "TSAsExpression".to_owned(),
        "ExportDeclaration" => "ExportNamedDeclaration".to_owned(),
        "FormalParameterRest" => "RestElement".to_owned(),
        "SuperPropExpression" => "MemberExpression".to_owned(),
        "TsThisParameter" => "Identifier".to_owned(),
        "ClassProperty" => "PropertyDefinition".to_owned(),
        "TSGetterSignature" | "TSSetterSignature" | "TsGetterSignature" | "TsSetterSignature" => {
            "TSMethodSignature".to_owned()
        }
        "Directive" => "ExpressionStatement".to_owned(),
        "ParenthesisExpression" => "ParenthesizedExpression".to_owned(),
        "FunctionBody" => "BlockStatement".to_owned(),
        "TSBigintKeyword" => "TSBigIntKeyword".to_owned(),
        "TSExpressionWithTypeArguments" | "TsExpressionWithTypeArguments" => {
            "TSInterfaceHeritage".to_owned()
        }
        "IdentifierReference" | "BindingIdentifier" | "IdentifierName" | "LabelIdentifier" => {
            "Identifier".to_owned()
        }
        name if name.starts_with("Ts") => format!("TS{}", &name[2..]),
        _ => name.to_owned(),
    }
}

fn json_type_name(object: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    let Some(name) = object.get("type").and_then(serde_json::Value::as_str) else {
        return (object.get("expression").is_some()
            && object.get("spread").is_some_and(|spread| !spread.is_null()))
        .then(|| "SpreadElement".to_owned());
    };
    if name == "FunctionDeclaration"
        && (object.get("declare").and_then(serde_json::Value::as_bool) == Some(true)
            || object.get("body").map_or(true, serde_json::Value::is_null))
    {
        return Some("TSDeclareFunction".to_owned());
    }
    if name == "OptionalChainingExpression" {
        return Some("ChainExpression".to_owned());
    }
    if name == "TsTupleElement" {
        return object
            .get("label")
            .filter(|label| label.is_object())
            .map(|_| "TSNamedTupleMember".to_owned());
    }
    if matches!(name, "Computed" | "Parameter") {
        return None;
    }
    if name == "BinaryExpression"
        && matches!(
            object.get("operator").and_then(serde_json::Value::as_str),
            Some("&&" | "||" | "??")
        )
    {
        return Some("LogicalExpression".to_owned());
    }
    if name == "TsLiteralType"
        && object
            .get("literal")
            .and_then(serde_json::Value::as_object)
            .and_then(|literal| literal.get("type"))
            .and_then(serde_json::Value::as_str)
            == Some("TemplateLiteral")
    {
        return Some("TSTemplateLiteralType".to_owned());
    }
    if name == "TsKeywordType" {
        let kind = object.get("kind")?.as_str()?;
        let suffix = match kind {
            "any" => "Any",
            "bigint" => "BigInt",
            "boolean" => "Boolean",
            "intrinsic" => "Intrinsic",
            "never" => "Never",
            "null" => "Null",
            "number" => "Number",
            "object" => "Object",
            "string" => "String",
            "symbol" => "Symbol",
            "undefined" => "Undefined",
            "unknown" => "Unknown",
            "void" => "Void",
            _ => return Some(normalize_type_name(name)),
        };
        return Some(format!("TS{suffix}Keyword"));
    }
    Some(normalize_type_name(name))
}

fn json_span(object: &serde_json::Map<String, serde_json::Value>) -> Option<(u32, u32)> {
    let numeric_pair = |start: &serde_json::Value, end: &serde_json::Value| {
        Some((
            u32::try_from(start.as_u64()?).ok()?,
            u32::try_from(end.as_u64()?).ok()?,
        ))
    };
    if let (Some(start), Some(end)) = (object.get("start"), object.get("end")) {
        return numeric_pair(start, end);
    }
    if let Some(serde_json::Value::Object(span)) = object.get("span") {
        if let (Some(start), Some(end)) = (span.get("start"), span.get("end")) {
            return numeric_pair(start, end);
        }
        if let (Some(start), Some(end)) = (span.get("lo"), span.get("hi")) {
            return numeric_pair(start, end);
        }
    }
    if let Some(serde_json::Value::Array(range)) = object.get("range") {
        if let [start, end] = range.as_slice() {
            return numeric_pair(start, end);
        }
    }
    if matches!(
        object.get("type").and_then(serde_json::Value::as_str),
        Some("ObjectProperty" | "KeyValueProperty" | "KeyValuePatternProperty")
    ) {
        let key = object.get("key")?.as_object()?;
        let value = object.get("value")?.as_object()?;
        let key_span = json_span(key)?;
        let value_span = json_span(value)?;
        return Some((key_span.0, value_span.1));
    }
    if let (Some(spread), Some(expression)) = (
        object.get("spread").and_then(serde_json::Value::as_object),
        object
            .get("expression")
            .or_else(|| object.get("arguments"))
            .and_then(serde_json::Value::as_object),
    ) {
        let spread_span = json_span(spread)?;
        let expression_span = json_span(expression)?;
        return Some((spread_span.0, expression_span.1));
    }
    None
}

fn canonical_span(
    object: &serde_json::Map<String, serde_json::Value>,
    source: &str,
) -> Option<(u32, u32)> {
    let kind = json_type_name(object)?;
    let span = json_span(object)?;
    if kind == "Program" {
        return Some((0, u32::try_from(source.len()).ok()?));
    }
    if object.get("type").and_then(serde_json::Value::as_str) == Some("TsThisParameter") {
        return object
            .get("thisSpan")
            .and_then(serde_json::Value::as_object)
            .and_then(json_span);
    }
    if kind == "ChainExpression"
        && matches!(
            object.get("type").and_then(serde_json::Value::as_str),
            Some("ChainExpression" | "OptionalChainingExpression")
        )
        && span.1 > 0
        && source.get(span.1 as usize - 1..span.1 as usize) == Some("!")
    {
        return Some((span.0, span.1 - 1));
    }
    if kind == "RestElement" {
        if let Some(rest_span) = object
            .get("rest")
            .and_then(serde_json::Value::as_object)
            .and_then(json_span)
        {
            return Some((rest_span.0, span.1));
        }
    }
    if kind != "Identifier" {
        return Some(span);
    }

    let Some(name) = object
        .get("name")
        .or_else(|| object.get("value"))
        .and_then(serde_json::Value::as_str)
    else {
        return Some(span);
    };
    let Ok(name_len) = u32::try_from(name.len()) else {
        return Some(span);
    };
    let Some(token_end) = span.0.checked_add(name_len) else {
        return Some(span);
    };
    let Some(token_range) = source.get(span.0 as usize..token_end as usize) else {
        return Some(span);
    };
    if token_range != name {
        return Some(span);
    }

    let optional_end = if object.get("optional").and_then(serde_json::Value::as_bool) == Some(true)
        && source.get(token_end as usize..token_end as usize + 1) == Some("?")
    {
        token_end + 1
    } else {
        token_end
    };
    let Some(annotation) = object
        .get("typeAnnotation")
        .and_then(serde_json::Value::as_object)
    else {
        return Some(if optional_end != token_end {
            (span.0, optional_end)
        } else {
            span
        });
    };
    let Some((annotation_start, annotation_end)) = json_span(annotation) else {
        return Some(span);
    };
    if annotation_start == optional_end
        && (annotation_end == span.1 || span.1 == token_end || span.1 == optional_end)
    {
        Some((span.0, optional_end))
    } else {
        Some(span)
    }
}

fn const_assertion_identifier_span(
    object: &serde_json::Map<String, serde_json::Value>,
    source: &str,
) -> Option<(u32, u32)> {
    let assertion_span = json_span(object)?;
    let expression_span = json_span(object.get("expression")?.as_object()?)?;
    let suffix = source.get(expression_span.1 as usize..assertion_span.1 as usize)?;
    let start = expression_span
        .1
        .checked_add(u32::try_from(suffix.rfind("const")?).ok()?)?;
    let end = start.checked_add(5)?;
    if source.get(start as usize..end as usize)? != "const"
        || !source
            .get(end as usize..assertion_span.1 as usize)?
            .trim()
            .is_empty()
        || source[..start as usize]
            .chars()
            .next_back()
            .is_some_and(|character| {
                character == '_' || character == '$' || character.is_alphanumeric()
            })
        || source[end as usize..]
            .chars()
            .next()
            .is_some_and(|character| {
                character == '_' || character == '$' || character.is_alphanumeric()
            })
    {
        return None;
    }
    Some((start, end))
}

fn template_quasi_span(
    template: &serde_json::Map<String, serde_json::Value>,
    index: usize,
    source: &str,
) -> Option<(u32, u32)> {
    let span = json_span(template)?;
    let quasis = template.get("quasis")?.as_array()?;
    let expressions = template
        .get("expressions")
        .or_else(|| template.get("types"))?
        .as_array()?;
    if quasis.len() != expressions.len() + 1 {
        return None;
    }

    let bytes = source.as_bytes();
    let (start, end) = span;
    if bytes.get(start as usize) != Some(&b'`')
        || bytes.get(end.checked_sub(1)? as usize) != Some(&b'`')
    {
        return None;
    }

    let mut cursor = start.checked_add(1)?;
    for (expression_index, expression) in expressions.iter().enumerate() {
        let expression_span = json_span(expression.as_object()?)?;
        let open = (cursor..expression_span.0.saturating_sub(1)).find(|&offset| {
            bytes.get(offset as usize..offset as usize + 2) == Some(b"${")
                && !is_escaped_template_delimiter(bytes, offset as usize)
                && is_template_trivia(bytes, offset as usize + 2, expression_span.0 as usize)
        })?;
        let raw = template_raw(quasis.get(expression_index)?.as_object()?)?;
        if source.get(cursor as usize..open as usize)? != raw {
            return None;
        }
        if index == expression_index {
            return Some((cursor, open));
        }

        let close = skip_template_trivia(bytes, expression_span.1 as usize)?;
        if bytes.get(close) != Some(&b'}') {
            return None;
        }
        cursor = u32::try_from(close).ok()?.checked_add(1)?;
    }

    let raw = template_raw(quasis.get(index)?.as_object()?)?;
    if index != expressions.len() {
        return None;
    }
    let quasi_start = cursor;
    let quasi_end = end.checked_sub(1)?;
    (source.get(quasi_start as usize..quasi_end as usize)? == raw)
        .then_some((quasi_start, quasi_end))
}

fn template_raw(quasi: &serde_json::Map<String, serde_json::Value>) -> Option<&str> {
    quasi
        .get("raw")
        .or_else(|| quasi.get("value")?.get("raw"))?
        .as_str()
}

fn is_escaped_template_delimiter(bytes: &[u8], offset: usize) -> bool {
    let mut backslashes = 0;
    let mut index = offset;
    while index > 0 && bytes[index - 1] == b'\\' {
        backslashes += 1;
        index -= 1;
    }
    backslashes % 2 == 1
}

fn is_template_trivia(bytes: &[u8], start: usize, end: usize) -> bool {
    skip_template_trivia(bytes, start).is_some_and(|offset| offset == end)
}

fn skip_template_trivia(bytes: &[u8], mut offset: usize) -> Option<usize> {
    loop {
        while bytes.get(offset).is_some_and(u8::is_ascii_whitespace) {
            offset += 1;
        }
        if bytes.get(offset..offset + 2) == Some(b"//") {
            offset += 2;
            while bytes
                .get(offset)
                .is_some_and(|byte| !matches!(byte, b'\n' | b'\r'))
            {
                offset += 1;
            }
        } else if bytes.get(offset..offset + 2) == Some(b"/*") {
            let relative_end = bytes[offset + 2..]
                .windows(2)
                .position(|pair| pair == b"*/")?;
            offset += relative_end + 4;
        } else {
            return Some(offset);
        }
    }
}

fn collect_synthetic_body_spans(
    value: &serde_json::Value,
    source: &str,
    spans: &mut SyntheticBodySpanMap,
) {
    match value {
        serde_json::Value::Object(object) => {
            let node_type = object.get("type").and_then(serde_json::Value::as_str);
            let expected_body_type =
                if matches!(node_type, Some("ClassDeclaration" | "ClassExpression")) {
                    Some("ClassBody")
                } else if node_type == Some("TSEnumDeclaration") {
                    Some("TSEnumBody")
                } else {
                    None
                };
            if expected_body_type.is_some() {
                if let (Some(parent_span), Some(body)) = (
                    json_span(object),
                    object.get("body").and_then(serde_json::Value::as_object),
                ) {
                    if body.get("type").and_then(serde_json::Value::as_str) == expected_body_type {
                        if let Some(body_span) = json_span(body) {
                            let valid_range = parent_span.0 <= body_span.0
                                && body_span.1 <= parent_span.1
                                && source
                                    .get(body_span.0 as usize..body_span.1 as usize)
                                    .is_some_and(|text| {
                                        text.starts_with('{') && text.ends_with('}')
                                    });
                            if valid_range {
                                spans.insert(parent_span, body_span);
                            }
                        }
                    }
                }
            }
            for child in object.values() {
                collect_synthetic_body_spans(child, source, spans);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                collect_synthetic_body_spans(child, source, spans);
            }
        }
        _ => {}
    }
}

fn add_offset(span: (u32, u32), adjustment: i64) -> (u32, u32) {
    let adjust = |value: u32| (i64::from(value) + adjustment).max(0) as u32;
    (adjust(span.0), adjust(span.1))
}

fn collect_json_shape(
    value: &serde_json::Value,
    shape: &mut AstShape,
    offset_adjustment: i64,
    path: &str,
    source: &str,
    parent_type: Option<&str>,
    parent_key: Option<&str>,
    parent_is_method: bool,
    parent_node: Option<&serde_json::Value>,
    synthetic_body_spans: Option<&SyntheticBodySpanMap>,
    within_optional_chain: bool,
) {
    match value {
        serde_json::Value::Object(object) => {
            let is_optional_chain = object.get("type").and_then(serde_json::Value::as_str)
                == Some("OptionalChainingExpression");
            if let Some(mut kind) = json_type_name(object) {
                if parent_key == Some("implements") && kind == "TSInterfaceHeritage" {
                    kind = "TSClassImplements".to_owned();
                }
                let inline_method_function = kind == "FunctionExpression"
                    && ((parent_type == Some("MethodDefinition") && parent_key == Some("value"))
                        || (parent_type == Some("Property")
                            && parent_key == Some("value")
                            && parent_is_method));
                let inline_template_literal = kind == "TemplateLiteral"
                    && parent_type == Some("TsLiteralType")
                    && parent_key == Some("literal");
                let negative_numeric_type = kind == "Literal"
                    && parent_type == Some("TsLiteralType")
                    && parent_key == Some("literal")
                    && object
                        .get("raw")
                        .and_then(serde_json::Value::as_str)
                        .zip(json_span(object))
                        .is_some_and(|(raw, span)| {
                            raw.starts_with('-')
                                && raw.len() > 1
                                && (source.get(span.0 as usize..span.1 as usize) == Some(raw)
                                    || (span.0 > 0
                                        && source.get(span.0 as usize - 1..span.1 as usize)
                                            == Some(raw)))
                        });
                let nested_optional_chain = is_optional_chain && within_optional_chain;
                if negative_numeric_type {
                    let span = json_span(object).expect("validated negative literal span");
                    let unary_start = if source.get(span.0 as usize..span.1 as usize)
                        == object.get("raw").and_then(serde_json::Value::as_str)
                    {
                        span.0
                    } else {
                        span.0 - 1
                    };
                    let unary_span = (unary_start, span.1);
                    *shape.types.entry("UnaryExpression".to_owned()).or_default() += 1;
                    shape
                        .type_paths
                        .insert(path.to_owned(), "UnaryExpression".to_owned());
                    let (start, end) = add_offset(unary_span, offset_adjustment);
                    *shape
                        .spans
                        .entry(("UnaryExpression".to_owned(), start, end))
                        .or_default() += 1;
                    shape.span_paths.insert(
                        format!("{path}.span"),
                        ("UnaryExpression".to_owned(), start, end),
                    );

                    let argument_span = (unary_start + 1, span.1);
                    *shape.types.entry("Literal".to_owned()).or_default() += 1;
                    let (start, end) = add_offset(argument_span, offset_adjustment);
                    *shape
                        .spans
                        .entry(("Literal".to_owned(), start, end))
                        .or_default() += 1;
                    shape
                        .type_paths
                        .insert(format!("{path}.argument"), "Literal".to_owned());
                    shape.span_paths.insert(
                        format!("{path}.argument.span"),
                        ("Literal".to_owned(), start, end),
                    );
                } else if !nested_optional_chain
                    && !inline_method_function
                    && !inline_template_literal
                {
                    *shape.types.entry(kind.clone()).or_default() += 1;
                    shape.type_paths.insert(path.to_owned(), kind.clone());
                    let span = if kind == "TemplateElement" && parent_key == Some("quasis") {
                        let index = path
                            .rsplit_once('[')
                            .and_then(|(_, index)| index.strip_suffix(']'))
                            .and_then(|index| index.parse::<usize>().ok());
                        index
                            .and_then(|index| {
                                parent_node
                                    .and_then(serde_json::Value::as_object)
                                    .filter(|parent| {
                                        matches!(
                                            parent.get("type").and_then(serde_json::Value::as_str),
                                            Some(
                                                "TemplateLiteral"
                                                    | "TSTemplateLiteralType"
                                                    | "TsTemplateLiteralType"
                                            )
                                        )
                                    })
                                    .and_then(|parent| template_quasi_span(parent, index, source))
                            })
                            .or_else(|| canonical_span(object, source))
                    } else {
                        canonical_span(object, source)
                    };
                    if let Some(span) = span {
                        let (start, end) = add_offset(span, offset_adjustment);
                        *shape.spans.entry((kind.clone(), start, end)).or_default() += 1;
                        shape
                            .span_paths
                            .insert(format!("{path}.span"), (kind.clone(), start, end));
                    }
                }
                if object.get("type").and_then(serde_json::Value::as_str) == Some("ImportSpecifier")
                    && object
                        .get("imported")
                        .is_some_and(serde_json::Value::is_null)
                {
                    if let Some(span) = object
                        .get("local")
                        .and_then(serde_json::Value::as_object)
                        .and_then(|local| canonical_span(local, source))
                    {
                        *shape.types.entry("Identifier".to_owned()).or_default() += 1;
                        let (start, end) = add_offset(span, offset_adjustment);
                        *shape
                            .spans
                            .entry(("Identifier".to_owned(), start, end))
                            .or_default() += 1;
                        shape
                            .type_paths
                            .insert(format!("{path}.imported"), "Identifier".to_owned());
                        shape.span_paths.insert(
                            format!("{path}.imported.span"),
                            ("Identifier".to_owned(), start, end),
                        );
                    }
                }
                if object.get("type").and_then(serde_json::Value::as_str)
                    == Some("TsConstAssertion")
                {
                    if let Some(span) = const_assertion_identifier_span(object, source) {
                        *shape.types.entry("TSTypeReference".to_owned()).or_default() += 1;
                        let (start, end) = add_offset(span, offset_adjustment);
                        *shape
                            .spans
                            .entry(("TSTypeReference".to_owned(), start, end))
                            .or_default() += 1;
                        shape.type_paths.insert(
                            format!("{path}.typeAnnotation"),
                            "TSTypeReference".to_owned(),
                        );
                        shape.span_paths.insert(
                            format!("{path}.typeAnnotation.span"),
                            ("TSTypeReference".to_owned(), start, end),
                        );
                        *shape.types.entry("Identifier".to_owned()).or_default() += 1;
                        *shape
                            .spans
                            .entry(("Identifier".to_owned(), start, end))
                            .or_default() += 1;
                        shape.type_paths.insert(
                            format!("{path}.typeAnnotation.typeName"),
                            "Identifier".to_owned(),
                        );
                        shape.span_paths.insert(
                            format!("{path}.typeAnnotation.typeName.span"),
                            ("Identifier".to_owned(), start, end),
                        );
                    }
                }
                if object.get("type").and_then(serde_json::Value::as_str)
                    == Some("AssignmentPatternProperty")
                {
                    *shape.types.entry("Identifier".to_owned()).or_default() += 1;
                    if let Some(key) = object.get("key").and_then(serde_json::Value::as_object) {
                        if let Some(span) = canonical_span(key, source) {
                            let (start, end) = add_offset(span, offset_adjustment);
                            *shape
                                .spans
                                .entry(("Identifier".to_owned(), start, end))
                                .or_default() += 1;
                        }
                    }
                    if object.get("value").is_some_and(|value| !value.is_null()) {
                        *shape
                            .types
                            .entry("AssignmentPattern".to_owned())
                            .or_default() += 1;
                        if let Some(span) = canonical_span(object, source) {
                            let (start, end) = add_offset(span, offset_adjustment);
                            *shape
                                .spans
                                .entry(("AssignmentPattern".to_owned(), start, end))
                                .or_default() += 1;
                        }
                    }
                }
                if kind == "Identifier"
                    && parent_key == Some("properties")
                    && matches!(parent_type, Some("ObjectExpression" | "ObjectPattern"))
                {
                    *shape.types.entry("Property".to_owned()).or_default() += 1;
                    *shape.types.entry("Identifier".to_owned()).or_default() += 1;
                    if let Some(span) = canonical_span(object, source) {
                        let (start, end) = add_offset(span, offset_adjustment);
                        *shape
                            .spans
                            .entry(("Property".to_owned(), start, end))
                            .or_default() += 1;
                        *shape
                            .spans
                            .entry(("Identifier".to_owned(), start, end))
                            .or_default() += 1;
                    }
                }
                if matches!(
                    object.get("type").and_then(serde_json::Value::as_str),
                    Some("ClassDeclaration" | "ClassExpression")
                ) && object.get("body").is_some_and(serde_json::Value::is_array)
                {
                    if let Some(class_span) = json_span(object) {
                        if let Some(body_span) =
                            synthetic_body_spans.and_then(|spans| spans.get(&class_span))
                        {
                            let (start, end) = add_offset(*body_span, offset_adjustment);
                            *shape.types.entry("ClassBody".to_owned()).or_default() += 1;
                            *shape
                                .spans
                                .entry(("ClassBody".to_owned(), start, end))
                                .or_default() += 1;
                            shape.span_paths.insert(
                                format!("{path}.body.span"),
                                ("ClassBody".to_owned(), start, end),
                            );
                        }
                    }
                }
                if object.get("type").and_then(serde_json::Value::as_str)
                    == Some("TsEnumDeclaration")
                    && object
                        .get("members")
                        .is_some_and(serde_json::Value::is_array)
                {
                    if let Some(enum_span) = json_span(object) {
                        if let Some(body_span) =
                            synthetic_body_spans.and_then(|spans| spans.get(&enum_span))
                        {
                            let (start, end) = add_offset(*body_span, offset_adjustment);
                            *shape.types.entry("TSEnumBody".to_owned()).or_default() += 1;
                            *shape
                                .spans
                                .entry(("TSEnumBody".to_owned(), start, end))
                                .or_default() += 1;
                            shape
                                .type_paths
                                .insert(format!("{path}.body"), "TSEnumBody".to_owned());
                            shape.span_paths.insert(
                                format!("{path}.body.span"),
                                ("TSEnumBody".to_owned(), start, end),
                            );
                        }
                    }
                }
            }
            for (key, child) in object {
                let node_type = object.get("type").and_then(serde_json::Value::as_str);
                let child_within_optional_chain = (is_optional_chain && key == "base")
                    || (within_optional_chain
                        && matches!(
                            (node_type, key.as_str()),
                            (Some("MemberExpression"), "object")
                                | (Some("CallExpression"), "callee")
                        ));
                collect_json_shape(
                    child,
                    shape,
                    offset_adjustment,
                    &format!("{path}.{key}"),
                    source,
                    object.get("type").and_then(serde_json::Value::as_str),
                    Some(key),
                    object.get("method").and_then(serde_json::Value::as_bool) == Some(true)
                        || matches!(
                            object.get("kind").and_then(serde_json::Value::as_str),
                            Some("get" | "set")
                        ),
                    Some(value),
                    synthetic_body_spans,
                    child_within_optional_chain,
                );
            }
        }
        serde_json::Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect_json_shape(
                    child,
                    shape,
                    offset_adjustment,
                    &format!("{path}[{index}]"),
                    source,
                    parent_type,
                    parent_key,
                    parent_is_method,
                    parent_node,
                    synthetic_body_spans,
                    within_optional_chain,
                );
            }
        }
        _ => {}
    }
}

fn json_ast_shape(
    value: &serde_json::Value,
    offset_adjustment: i64,
    source: &str,
    synthetic_body_spans: Option<&SyntheticBodySpanMap>,
) -> AstShape {
    let mut shape = AstShape::default();
    collect_json_shape(
        value,
        &mut shape,
        offset_adjustment,
        "$",
        source,
        None,
        None,
        false,
        None,
        synthetic_body_spans,
        false,
    );
    shape
}

fn first_span_path_difference(
    reference: &AstShape,
    actual: &AstShape,
) -> Option<(String, String, (u32, u32), (u32, u32))> {
    reference
        .span_paths
        .iter()
        .find_map(|(path, (kind, start, end))| {
            let (actual_kind, actual_start, actual_end) = actual.span_paths.get(path)?;
            (kind == actual_kind && (start, end) != (actual_start, actual_end)).then(|| {
                (
                    path.clone(),
                    kind.clone(),
                    (*start, *end),
                    (*actual_start, *actual_end),
                )
            })
        })
}

fn histogram_mismatches<K: Ord>(
    reference: &BTreeMap<K, usize>,
    actual: &BTreeMap<K, usize>,
    limit: usize,
    mut describe: impl FnMut(&K, usize, usize) -> String,
) -> Vec<String> {
    let keys: std::collections::BTreeSet<_> = reference.keys().chain(actual.keys()).collect();
    keys.into_iter()
        .filter_map(|key| {
            let expected = reference.get(key).copied().unwrap_or_default();
            let found = actual.get(key).copied().unwrap_or_default();
            (expected != found).then(|| describe(key, expected, found))
        })
        .take(limit)
        .collect()
}

fn compare_shape(path: &str, reference: &AstShape, actual: &AstShape, label: &str) {
    let mut mismatches = Vec::new();
    if let Some((node_path, expected, found)) =
        reference
            .type_paths
            .iter()
            .find_map(|(node_path, expected)| {
                actual
                    .type_paths
                    .get(node_path)
                    .filter(|found| *found != expected)
                    .map(|found| (node_path, expected, found))
            })
    {
        mismatches.push(format!(
            "node type mismatch at {node_path}: Parser A expected {expected}, {label} actual {found}"
        ));
    }
    if let Some((node_path, kind, expected, found)) = first_span_path_difference(reference, actual)
    {
        mismatches.push(format!(
            "span mismatch at {node_path} ({kind}): Parser A expected [{}, {}], {label} actual [{}, {}]",
            expected.0, expected.1, found.0, found.1
        ));
    }
    mismatches.extend(histogram_mismatches(
        &reference.types,
        &actual.types,
        2,
        |kind, expected, found| {
            format!("node type {kind}: Parser A expected {expected}, {label} actual {found}")
        },
    ));
    mismatches.extend(histogram_mismatches(
        &reference.spans,
        &actual.spans,
        2,
        |span, expected, found| {
            format!(
                "span {} at [{}, {}]: Parser A expected {expected}, {label} actual {found}",
                span.0, span.1, span.2
            )
        },
    ));
    assert!(
        mismatches.is_empty(),
        "AST shape mismatch for {path}: {}",
        mismatches.join("; ")
    );
}

fn measure(mut sample: impl FnMut() -> u64) -> Stats {
    for _ in 0..WARMUP {
        sample();
    }
    let mut secs: Vec<f64> = (0..RUNS).map(|_| sample() as f64 / 1e9).collect();
    secs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Stats {
        median: secs[RUNS / 2],
        min: secs[0],
        p99: secs[RUNS * 99 / 100],
    }
}

fn swc_syntax(path: &str) -> Syntax {
    if path.ends_with(".tsx") {
        Syntax::Typescript(TsSyntax {
            tsx: true,
            dts: false,
            ..Default::default()
        })
    } else if path.ends_with(".ts") {
        Syntax::Typescript(TsSyntax {
            dts: path.ends_with(".d.ts"),
            ..Default::default()
        })
    } else if path.ends_with(".jsx") {
        Syntax::Es(EsSyntax {
            jsx: true,
            ..Default::default()
        })
    } else {
        Syntax::Es(EsSyntax::default())
    }
}

fn swc_next_options(path: &str) -> Options {
    Options {
        lang: Lang::from_path(path),
        source_type: if path.ends_with(".d.ts") {
            SwcNextSourceType::Script
        } else {
            SwcNextSourceType::Module
        },
        ..Options::default()
    }
}

fn bench_oxc(src: &str, st: SourceType) -> Stats {
    measure(|| {
        let allocator = Allocator::default();
        let start = Instant::now();
        let ret = OxcParser::new(&allocator, black_box(src), st).parse();
        let dt = start.elapsed().as_nanos() as u64;
        assert!(
            ret.diagnostics.is_empty(),
            "Parser A diagnostics: {:?}",
            ret.diagnostics
        );
        black_box(&ret);
        dt
    })
}

fn bench_oxc_semantic(src: &str, st: SourceType) -> Stats {
    measure(|| {
        let allocator = Allocator::default();
        let start = Instant::now();
        let ret = OxcParser::new(&allocator, black_box(src), st).parse();
        let sem = SemanticBuilder::new()
            .with_check_syntax_error(true)
            .build(&ret.program);
        let dt = start.elapsed().as_nanos() as u64;
        assert!(
            ret.diagnostics.is_empty(),
            "Parser A diagnostics: {:?}",
            ret.diagnostics
        );
        assert!(
            sem.diagnostics.is_empty(),
            "Parser A semantic diagnostics: {:?}",
            sem.diagnostics
        );
        black_box(&sem);
        black_box(&ret);
        dt
    })
}

fn bench_swc(src: &str, path: &str) -> Stats {
    let syntax = swc_syntax(path);
    let is_declaration_file = path.ends_with(".d.ts");
    measure(|| {
        let input = StringInput::new(src, BytePos(0), BytePos(src.len() as u32));
        let start = Instant::now();
        let mut parser = SwcParser::new(syntax, black_box(input), None);
        if is_declaration_file {
            parser.parse_script().map(|script| {
                let errors = parser.take_errors();
                let dt = start.elapsed().as_nanos() as u64;
                assert!(errors.is_empty(), "Parser B diagnostics: {errors:?}");
                black_box(&script);
                dt
            })
        } else {
            parser.parse_module().map(|module| {
                let errors = parser.take_errors();
                let dt = start.elapsed().as_nanos() as u64;
                assert!(errors.is_empty(), "Parser B diagnostics: {errors:?}");
                black_box(&module);
                dt
            })
        }
        .unwrap_or_else(|error| panic!("Parser B failed: {error:?}"))
    })
}

fn bench_swc_next(src: &str, path: &str, semantic: bool) -> Stats {
    let options = swc_next_options(path);
    measure(|| {
        let allocator = SwcNextAllocator::new();
        let start = Instant::now();
        let ret =
            SwcNextParser::init(&allocator, black_box(src), options, NoTokenParserConfig).parse();
        let sem = semantic.then(|| {
            analyze(
                &ret.ast,
                AnalyzeOptions {
                    check_syntax: SyntaxChecks::all(),
                    ..Default::default()
                },
            )
        });
        let dt = start.elapsed().as_nanos() as u64;
        black_box(&ret);
        black_box(&sem);
        assert!(
            ret.diagnostics.is_empty(),
            "Parser C diagnostics: {:?}",
            ret.diagnostics
        );
        if let Some(sem) = &sem {
            assert!(
                sem.diagnostics.is_empty(),
                "Parser C semantic diagnostics: {:?}",
                sem.diagnostics
            );
        }
        dt
    })
}

fn preflight(src: &str, path: &str, expected_oxc: &serde_json::Value) {
    let st = parse_config::source_type(path);
    let allocator = Allocator::default();
    let parsed = OxcParser::new(&allocator, src, st).parse();
    assert!(
        parsed.diagnostics.is_empty(),
        "Parser A diagnostics for {path}: {:?}",
        parsed.diagnostics
    );
    let serialized_oxc = parsed.program.to_estree_json(true, true);
    let serialized_oxc: serde_json::Value =
        serde_json::from_str(&serialized_oxc).expect("Parser A AST serialization failed");
    assert_eq!(
        &serialized_oxc, expected_oxc,
        "Oxc AST cache differs from current Oxc output for {path}; run `bun run generate:ast-cache`"
    );
    let mut synthetic_body_spans = SyntheticBodySpanMap::default();
    collect_synthetic_body_spans(expected_oxc, src, &mut synthetic_body_spans);
    let oxc_shape = json_ast_shape(expected_oxc, 0, src, None);
    let semantic = SemanticBuilder::new()
        .with_check_syntax_error(true)
        .build(&parsed.program);
    assert!(
        semantic.diagnostics.is_empty(),
        "Parser A semantic diagnostics for {path}: {:?}",
        semantic.diagnostics
    );

    let input = StringInput::new(src, BytePos(0), BytePos(src.len() as u32));
    let mut swc_parser = SwcParser::new(swc_syntax(path), input, None);
    let serialized_swc = if path.ends_with(".d.ts") {
        let script = swc_parser
            .parse_script()
            .unwrap_or_else(|error| panic!("Parser B failed for {path}: {error:?}"));
        serde_json::to_value(&script).expect("Parser B AST serialization failed")
    } else {
        let module = swc_parser
            .parse_module()
            .unwrap_or_else(|error| panic!("Parser B failed for {path}: {error:?}"));
        serde_json::to_value(&module).expect("Parser B AST serialization failed")
    };
    let swc_errors = swc_parser.take_errors();
    assert!(
        swc_errors.is_empty(),
        "Parser B diagnostics for {path}: {swc_errors:?}"
    );
    let swc_shape = json_ast_shape(&serialized_swc, 0, src, Some(&synthetic_body_spans));

    let allocator = SwcNextAllocator::new();
    let parsed =
        SwcNextParser::init(&allocator, src, swc_next_options(path), NoTokenParserConfig).parse();
    assert!(
        parsed.diagnostics.is_empty(),
        "Parser C diagnostics for {path}: {:?}",
        parsed.diagnostics
    );
    let mut next_shape = SwcNextAstShape {
        ast: &parsed.ast,
        source: src,
        shape: AstShape::default(),
        parents: Vec::new(),
    };
    parsed.ast.root_program().visit_with(&mut next_shape);
    let semantic = analyze(
        &parsed.ast,
        AnalyzeOptions {
            check_syntax: SyntaxChecks::all(),
            ..Default::default()
        },
    );
    assert!(
        semantic.diagnostics.is_empty(),
        "Parser C semantic diagnostics for {path}: {:?}",
        semantic.diagnostics
    );
    compare_shape(path, &oxc_shape, &swc_shape, "Parser B");
    compare_shape(path, &oxc_shape, &next_shape.shape, "Parser C");
}

fn preflight_files(paths: &[String]) {
    let cache = ast_cache::load_ast_cache();
    for path in paths {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("cannot read preflight input {path}: {error}"));
        let expected_oxc = ast_cache::cached_oxc_ast(&cache, &source, path);
        preflight(&source, path, expected_oxc);
    }
}

#[cfg(test)]
fn run_after_preflight<T>(gate: impl FnOnce(), timed: impl FnOnce() -> T) -> T {
    gate();
    timed()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .first()
        .is_some_and(|arg| arg == "--generate-ast-cache")
    {
        ast_cache::generate_ast_cache(&args[1..]);
        return;
    }
    let preflight_only = args.first().is_some_and(|arg| arg == "--preflight");
    let paths = if preflight_only {
        &args[1..]
    } else {
        &args[..]
    };
    if !paths.is_empty() {
        preflight_files(paths);
    }
    if preflight_only {
        return;
    }

    let mut out = String::from("{\"results\":[");
    let mut first = true;

    for path in paths {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("rust: cannot read {path}: {e}");
                continue;
            }
        };
        let st = parse_config::source_type(path);
        let cases = [
            ("oxc", bench_oxc(&source, st)),
            ("oxc_semantic", bench_oxc_semantic(&source, st)),
            ("swc", bench_swc(&source, path)),
            ("swc_next", bench_swc_next(&source, path, false)),
            ("swc_next_semantic", bench_swc_next(&source, path, true)),
        ];
        for (parser, s) in cases {
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!(
                "{{\"parser\":\"{parser}\",\"file\":\"{path}\",\"median\":{},\"min\":{},\"p99\":{}}}",
                s.median, s.min, s.p99
            ));
        }
    }
    out.push_str("]}");
    println!("{out}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    fn panic_message(result: std::thread::Result<()>) -> String {
        let payload = result.expect_err("expected preflight failure");
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|message| (*message).to_owned())
            })
            .unwrap_or_default()
    }

    #[test]
    fn declaration_file_uses_script_goal_across_native_parsers() {
        let path = "files/lib.dom.d.ts";
        assert!(parse_config::source_type(path).is_script());
        assert_eq!(
            swc_next_options(path).source_type,
            SwcNextSourceType::Script
        );
        assert!(matches!(
            swc_syntax(path),
            Syntax::Typescript(TsSyntax {
                dts: true,
                tsx: false,
                ..
            })
        ));
    }

    #[test]
    fn template_quasis_use_source_verified_raw_ranges() {
        let source = "const label = `name: ${value /* trailing */}`;";
        let start = source.find('`').unwrap() as u32;
        let end = source.rfind('`').unwrap() as u32 + 1;
        let expression_start = source.find("value").unwrap() as u32;
        let expression_end = expression_start + "value".len() as u32;
        let template = json!({
            "type": "TemplateLiteral",
            "span": { "start": start, "end": end },
            "quasis": [
                { "type": "TemplateElement", "raw": "name: " },
                { "type": "TemplateElement", "raw": "" }
            ],
            "expressions": [
                {
                    "type": "Identifier",
                    "span": { "start": expression_start, "end": expression_end }
                }
            ]
        });
        let template = template.as_object().unwrap();
        let open = source.find("${").unwrap() as u32;
        let close = source.rfind('}').unwrap() as u32;

        assert_eq!(
            template_quasi_span(template, 0, source),
            Some((start + 1, open))
        );
        assert_eq!(
            template_quasi_span(template, 1, source),
            Some((close + 1, end - 1))
        );
    }

    #[test]
    fn template_quasi_mismatch_fails_closed() {
        let source = "const label = `name: ${value}`;";
        let start = source.find('`').unwrap() as u32;
        let end = source.rfind('`').unwrap() as u32 + 1;
        let expression_start = source.find("value").unwrap() as u32;
        let template = json!({
            "type": "TemplateLiteral",
            "span": { "start": start, "end": end },
            "quasis": [
                { "type": "TemplateElement", "raw": "different" },
                { "type": "TemplateElement", "raw": "" }
            ],
            "expressions": [
                {
                    "type": "Identifier",
                    "span": { "start": expression_start, "end": expression_start + 5 }
                }
            ]
        });

        assert_eq!(
            template_quasi_span(template.as_object().unwrap(), 0, source),
            None
        );
    }

    #[test]
    fn preflight_succeeds_for_matching_smoke_profile() {
        let mut timed = false;
        let source = "const value: number = 1;";
        let expected_oxc = ast_cache::generate_oxc_ast(source, "smoke.ts");
        run_after_preflight(
            || preflight(source, "smoke.ts", &expected_oxc),
            || timed = true,
        );
        assert!(timed);
    }

    #[test]
    fn preflight_reports_parse_failures() {
        let failure = panic_message(catch_unwind(AssertUnwindSafe(|| {
            preflight("const value: = 1;", "invalid.ts", &serde_json::json!({}));
        })));
        assert!(failure.contains("Parser A diagnostics"));
        assert!(failure.contains("invalid.ts"));
    }

    #[test]
    fn shape_mismatch_reports_the_first_ast_path() {
        let mut expected = AstShape::default();
        expected.type_paths.insert(
            "$.body[0].expression".to_owned(),
            "ChainExpression".to_owned(),
        );
        let mut actual = AstShape::default();
        actual.type_paths.insert(
            "$.body[0].expression".to_owned(),
            "OptionalChainingExpression".to_owned(),
        );

        let mut timed = false;
        let failure = panic_message(catch_unwind(AssertUnwindSafe(|| {
            run_after_preflight(
                || compare_shape("fixture.ts", &expected, &actual, "Parser B"),
                || timed = true,
            );
        })));
        assert!(!timed);
        assert!(failure.contains(
            "node type mismatch at $.body[0].expression: Parser A expected ChainExpression, Parser B actual OptionalChainingExpression"
        ));
    }
}
