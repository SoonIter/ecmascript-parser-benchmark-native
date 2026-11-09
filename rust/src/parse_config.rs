use oxc_span::SourceType;

pub fn source_type(path: &str) -> SourceType {
    let source_type = SourceType::from_path(path)
        .unwrap_or_else(|error| panic!("invalid benchmark path {path}: {error}"));
    if path.ends_with(".d.ts") {
        return source_type.with_script(true);
    }

    match std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("js" | "jsx" | "ts" | "tsx") => source_type.with_module(true),
        _ => source_type,
    }
}

#[cfg(test)]
mod tests {
    use super::source_type;

    #[test]
    fn javascript_goal_is_module_without_jsx() {
        let source_type = source_type("files/react.js");

        assert!(source_type.is_module());
        assert!(!source_type.is_jsx());
    }

    #[test]
    fn jsx_goal_remains_enabled_for_jsx_files() {
        let source_type = source_type("files/component.jsx");

        assert!(source_type.is_module());
        assert!(source_type.is_jsx());
    }

    #[test]
    fn declaration_goal_is_script_without_jsx() {
        let source_type = source_type("files/lib.dom.d.ts");

        assert!(source_type.is_script());
        assert!(!source_type.is_module());
        assert!(source_type.is_typescript_definition());
        assert!(!source_type.is_jsx());
    }
}
