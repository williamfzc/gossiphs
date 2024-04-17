use crate::extractor::Extractor;

/*
tree-sitter query syntax
https://tree-sitter.github.io/tree-sitter/using-parsers#query-syntax
 */
pub struct Rule {
    // which symbols has been used (possibly imported) in this file
    pub(crate) import_grammar: &'static str,
    // which symbols has been exported from this file
    pub(crate) export_grammar: &'static str,
}

pub fn get_rule(extractor_type: &Extractor) -> Rule {
    match extractor_type {
        Extractor::Rust => Rule {
            import_grammar: r#"
(identifier) @variable_name
"#,
            export_grammar: r#"
(function_item name: (identifier) @exported_symbol)
"#,
        },

        Extractor::TypeScript => Rule {
            import_grammar: r#"
(identifier) @variable_name
"#,
            export_grammar: r#"
(function_declaration name: (identifier) @exported_symbol)
(arrow_function (identifier) @exported_symbol)
(generator_function_declaration name: (identifier) @exported_symbol)
(method_definition name: (property_identifier) @exported_symbol)
    "#,
        },
    }
}