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
(call_expression
  function: (identifier) @function)
(call_expression
  function: (field_expression
    field: (field_identifier) @function.method))
(call_expression
  function: (scoped_identifier
    "::"
    name: (identifier) @function))
"#,
            export_grammar: r#"
(function_item name: (identifier) @exported_symbol)
(function_signature_item name: (identifier) @exported_symbol)
(generic_function
  function: (identifier) @exported_symbol)
(generic_function
  function: (scoped_identifier
    name: (identifier) @exported_symbol))
"#,
        },

        Extractor::TypeScript => Rule {
            import_grammar: r#"
(identifier) @variable_name
"#,
            export_grammar: r#"
(export_statement (function_declaration name: (identifier) @exported_symbol))
(export_statement (arrow_function (identifier) @exported_symbol))
(export_statement (generator_function_declaration name: (identifier) @exported_symbol))
(method_definition name: (property_identifier) @exported_symbol)
(export_statement (type_alias_declaration name: (type_identifier) @exported_symbol))
(export_statement (interface_declaration name: (type_identifier) @exported_symbol))
(export_statement (class_declaration name: (type_identifier) @exported_symbol))
(export_specifier (identifier) @exported_symbol)
"#,
        },

        Extractor::Go => Rule {
            import_grammar: r#"
(identifier) @variable_name
"#,
            export_grammar: r#"
(function_declaration name: (identifier) @exported_symbol)
(method_declaration name: (field_identifier) @exported_symbol)
(type_alias name: (type_identifier) @exported_symbol)
(type_spec name: (type_identifier) @exported_symbol)
(const_spec name: (identifier) @exported_symbol)
(var_spec name: (identifier) @exported_symbol)
"#,
        },

        Extractor::Python => Rule {
            import_grammar: r#"
(identifier) @variable_name
"#,
            export_grammar: r#"
(function_definition name: (identifier) @exported_symbol)
(class_definition name: (identifier) @exported_symbol)
"#,
        },

        Extractor::JavaScript => Rule {
            import_grammar: r#"
(identifier) @variable_name
    "#,
            export_grammar: r#"
(function_declaration name: (identifier) @exported_symbol)
(class_declaration name: (identifier) @exported_symbol)
    "#,
        },
        Extractor::Java => Rule {
            import_grammar: r#"
((identifier) @variable_name)
  "#,
            // todo: not enough maybe
            export_grammar: r#"
(class_declaration name: (identifier) @exported_symbol)
  "#,
        },

        Extractor::Kotlin => Rule {
            import_grammar: r#"
(identifier (simple_identifier) @variable_name)
  "#,
            export_grammar: r#"
(class_declaration (type_identifier) @exported_symbol)
(function_declaration (simple_identifier) @exported_symbol)
  "#,
        },

        Extractor::Swift => Rule {
            import_grammar: r#"
((simple_identifier) @exported_symbol)
  "#,
            // TODO: not enough
            export_grammar: r#"
(function_declaration (simple_identifier) @method)
  "#,
        },
    }
}
