use std::hash::{Hash, Hasher};

use oxc_allocator::Allocator;
use oxc_parser::Parser as OxcParser;

use crate::parse_config::source_type;

const AST_CACHE_PATH: &str = ".cache/ecmascript-parser-benchmark-native/ast.json";
const AST_CACHE_VERSION: u64 = 1;

fn source_hash(source: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn generate_oxc_ast(src: &str, path: &str) -> serde_json::Value {
    let st = source_type(path);
    let allocator = Allocator::default();
    let parsed = OxcParser::new(&allocator, src, st).parse();
    assert!(
        parsed.diagnostics.is_empty(),
        "Oxc diagnostics while generating expected AST for {path}: {:?}",
        parsed.diagnostics
    );
    let serialized = parsed.program.to_estree_json(true, true);
    serde_json::from_str(&serialized).expect("Oxc AST serialization failed")
}

pub fn generate_ast_cache(paths: &[String]) {
    assert!(
        !paths.is_empty(),
        "expected files for Oxc AST cache generation"
    );
    let mut files = serde_json::Map::new();
    let cache_dir = ".cache/ecmascript-parser-benchmark-native";
    std::fs::create_dir_all(cache_dir).unwrap_or_else(|error| {
        panic!("cannot create Oxc AST cache directory {cache_dir}: {error}")
    });
    for path in paths {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("cannot read AST cache input {path}: {error}"));
        let ast = generate_oxc_ast(&source, path);
        files.insert(
            path.clone(),
            serde_json::json!({
                "source_hash": source_hash(&source),
                "ast": ast,
            }),
        );
    }
    let cache = serde_json::json!({
        "version": AST_CACHE_VERSION,
        "generated_by": "Oxc",
        "files": files,
    });
    let serialized = serde_json::to_string(&cache).expect("Oxc AST cache serialization failed");
    std::fs::write(AST_CACHE_PATH, format!("{serialized}\n"))
        .unwrap_or_else(|error| panic!("cannot write Oxc AST cache {AST_CACHE_PATH}: {error}"));
}

pub fn load_ast_cache() -> serde_json::Value {
    let contents = std::fs::read_to_string(AST_CACHE_PATH).unwrap_or_else(|error| {
        panic!(
            "cannot read Oxc AST cache {AST_CACHE_PATH}: {error}; run `bun run generate:ast-cache`"
        )
    });
    let cache: serde_json::Value =
        serde_json::from_str(&contents).expect("invalid Oxc AST cache JSON");
    assert_eq!(
        cache.get("version").and_then(serde_json::Value::as_u64),
        Some(AST_CACHE_VERSION),
        "unsupported Oxc AST cache version; run `bun run generate:ast-cache`"
    );
    assert_eq!(
        cache
            .get("generated_by")
            .and_then(serde_json::Value::as_str),
        Some("Oxc"),
        "Oxc AST cache has an unexpected source; run `bun run generate:ast-cache`"
    );
    cache
}

pub fn cached_oxc_ast<'a>(
    cache: &'a serde_json::Value,
    src: &str,
    path: &str,
) -> &'a serde_json::Value {
    let entry = cache
        .get("files")
        .and_then(|files| files.get(path))
        .unwrap_or_else(|| {
            panic!("Oxc AST cache has no entry for {path}; run `bun run generate:ast-cache`")
        });
    let expected_hash = source_hash(src);
    assert_eq!(
        entry.get("source_hash").and_then(serde_json::Value::as_str),
        Some(expected_hash.as_str()),
        "Oxc AST cache is stale for {path}; run `bun run generate:ast-cache`"
    );
    entry
        .get("ast")
        .filter(|ast| ast.is_object())
        .unwrap_or_else(|| panic!("Oxc AST cache entry is invalid for {path}; regenerate it"))
}
