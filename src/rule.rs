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

    // namespace control
    pub(crate) namespace_grammar: &'static str,
    pub(crate) namespace_filter_level: usize,
}

pub fn get_rule(extractor_type: &Extractor) -> Rule {
    match extractor_type {
        Extractor::Rust => Rule {
            import_grammar: r#"
(type_identifier) @variable_name
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
            namespace_grammar: r#"
(function_item) @body
(generic_function) @body
"#,
            namespace_filter_level: 1,
        },

        Extractor::TypeScript => Rule {
            import_grammar: r#"
(identifier) @variable_name
(type_identifier) @variable_name
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
(lexical_declaration (variable_declarator name: (identifier) @lexical_symbol))
"#,
            namespace_grammar: r#"
(class_declaration) @body
(function_declaration) @body
(interface_declaration) @body
(method_definition) @body
"#,
            namespace_filter_level: 1,
        },

        Extractor::Go => Rule {
            import_grammar: r#"
(identifier) @variable_name
(type_identifier) @variable_name
(field_identifier) @variable_name
"#,
            export_grammar: r#"
(function_declaration name: (identifier) @exported_symbol)
(method_declaration name: (field_identifier) @exported_symbol)
(type_alias name: (type_identifier) @exported_symbol)
(type_spec name: (type_identifier) @exported_symbol)
(const_spec name: (identifier) @exported_symbol)
(var_spec name: (identifier) @exported_symbol)
"#,
            namespace_grammar: r#"
(function_declaration) @body
(method_declaration) @body
"#,
            namespace_filter_level: 1,
        },

        Extractor::Python => Rule {
            import_grammar: r#"
(identifier) @variable_name
"#,
            export_grammar: r#"
(function_definition name: (identifier) @exported_symbol)
(class_definition name: (identifier) @exported_symbol)
"#,
            namespace_grammar: r#"
(function_definition) @body
(class_definition) @body
"#,
            namespace_filter_level: 2,
        },

        Extractor::JavaScript => Rule {
            import_grammar: r#"
(identifier) @variable_name
    "#,
            export_grammar: r#"
(function_declaration name: (identifier) @exported_symbol)
(class_declaration name: (identifier) @exported_symbol)
    "#,
            namespace_grammar: r#"
(function_declaration) @body
(class_declaration) @body
"#,
            namespace_filter_level: 2,
        },
        Extractor::Java => Rule {
            import_grammar: r#"
((identifier) @variable_name)
  "#,
            // todo: not enough maybe
            export_grammar: r#"
(class_declaration name: (identifier) @exported_symbol)
  "#,
            namespace_grammar: r#"
(class_declaration) @body
"#,
            namespace_filter_level: 1,
        },

        Extractor::Kotlin => Rule {
            import_grammar: r#"
(identifier (simple_identifier) @variable_name)
  "#,
            export_grammar: r#"
(class_declaration (type_identifier) @exported_symbol)
(function_declaration (simple_identifier) @exported_symbol)
  "#,
            namespace_grammar: "",
            namespace_filter_level: 0,
        },

        Extractor::Swift => Rule {
            import_grammar: r#"
((simple_identifier) @exported_symbol)
  "#,
            // TODO: not enough
            export_grammar: r#"
(function_declaration (simple_identifier) @method)
  "#,
            namespace_grammar: "",
            namespace_filter_level: 0,
        },

        Extractor::CSharp => Rule {
            // Basic C# rules might need refinement
            import_grammar: r#"
(using_directive name: (_) @import)
(identifier) @variable_name
"#,
            export_grammar: r#"
(class_declaration name: (identifier) @exported_symbol)
(interface_declaration name: (identifier) @exported_symbol)
(struct_declaration name: (identifier) @exported_symbol)
(enum_declaration name: (identifier) @exported_symbol)
(method_declaration name: (identifier) @exported_symbol)
(property_declaration name: (identifier) @exported_symbol)
(field_declaration (variable_declaration (variable_declarator (identifier) @exported_symbol)))
"#,
            namespace_grammar: r#"
(namespace_declaration) @body
(class_declaration) @body
(struct_declaration) @body
(interface_declaration) @body
"#,
            namespace_filter_level: 1,
        },
    }
}
