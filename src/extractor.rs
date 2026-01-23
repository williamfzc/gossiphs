/*
File: extractor.rs
Functionality: Symbol extraction from source code using tree-sitter.
Role: Implements language-specific symbol extraction logic for various programming languages, identifying definitions and references.
*/
use crate::rule::{get_rule, Rule};
use crate::symbol::Symbol;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tree_sitter::{Language, Parser, Query, QueryCursor};

pub enum Extractor {
    Rust,
    TypeScript,
    Go,
    Python,
    JavaScript,
    Java,
    Kotlin,
    Swift,
    CSharp,
}

const DEFAULT_NAMESPACE_REPR: &str = "<NS>";

impl Extractor {
    pub fn get_rule(&self) -> Rule {
        get_rule(self)
    }
    pub fn extract(&self, f: Arc<String>, s: &String) -> Vec<Symbol> {
        let lang = match self {
            Extractor::Rust => tree_sitter_rust::language(),
            Extractor::TypeScript => tree_sitter_typescript::language_typescript(),
            Extractor::Go => tree_sitter_go::language(),
            Extractor::Python => tree_sitter_python::language(),
            Extractor::JavaScript => tree_sitter_javascript::language(),
            Extractor::Java => tree_sitter_java::language(),
            Extractor::Kotlin => tree_sitter_kotlin::language(),
            Extractor::Swift => tree_sitter_swift::language(),
            Extractor::CSharp => tree_sitter_c_sharp::language(),
        };
        let result = self._extract(f.clone(), s, &lang);
        result.unwrap_or_else(|e| {
            tracing::error!("failed to extract symbols from {}: {}", f, e);
            Vec::new()
        })
    }

    fn _extract(&self, f: Arc<String>, s: &String, language: &Language) -> Result<Vec<Symbol>> {
        let mut parser = Parser::new();
        parser
            .set_language(language)
            .context("Error loading grammar")?;
        let tree = parser.parse(s, None).context("Error parsing code")?;

        let rule = get_rule(&self);
        let mut ret = Vec::new();
        let mut taken = HashMap::new();

        let mut name_cache: HashMap<String, Arc<String>> = HashMap::new();
        let mut get_shared_name = |name: String| -> Arc<String> {
            name_cache
                .entry(name.clone())
                .or_insert_with(|| Arc::new(name))
                .clone()
        };

        let filter_re = if let Some(re_str) = rule.exclude_regex {
            Some(regex::Regex::new(re_str).context("Invalid exclude_regex in rule")?)
        } else {
            None
        };

        let is_blacklisted = |name: &str| -> bool {
            if rule.blacklist.contains(&name) {
                return true;
            }
            if let Some(re) = &filter_re {
                if re.is_match(name) {
                    return true;
                }
            }
            false
        };

        // defs
        {
            let query = Query::new(language, rule.export_grammar).context("Error creating export query")?;
            let mut cursor = QueryCursor::new();
            let matches = cursor.matches(&query, tree.root_node(), s.as_bytes());
            for mat in matches {
                for capture in mat.captures {
                    let matched_node = capture.node;
                    let range = matched_node.range();

                    if let Ok(str_slice) = matched_node.utf8_text(s.as_bytes()) {
                        let string = str_slice.to_string();
                        if is_blacklisted(&string) {
                            continue;
                        }
                        let shared_name = get_shared_name(string);
                        let def_node = Symbol::new_def(f.clone(), shared_name, range);
                        taken.insert(def_node.id(), ());
                        ret.push(def_node);
                    }
                }
            }
        }

        // refs
        {
            let query = Query::new(language, rule.import_grammar).context("Error creating import query")?;
            let mut cursor = QueryCursor::new();
            let matches = cursor.matches(&query, tree.root_node(), s.as_bytes());
            for mat in matches {
                for capture in mat.captures {
                    let matched_node = capture.node;
                    let range = matched_node.range();

                    if let Ok(str_slice) = matched_node.utf8_text(s.as_bytes()) {
                        let string = str_slice.to_string();
                        if is_blacklisted(&string) {
                            continue;
                        }
                        let shared_name = get_shared_name(string);
                        let ref_node = Symbol::new_ref(f.clone(), shared_name, range);
                        if taken.contains_key(&ref_node.id()) {
                            continue;
                        }
                        ret.push(ref_node);
                    }
                }
            }
        }

        // namespace
        {
            if !rule.namespace_grammar.is_empty() {
                let query = Query::new(language, rule.namespace_grammar).context("Error creating namespace query")?;
                let mut cursor = QueryCursor::new();
                let matches = cursor.matches(&query, tree.root_node(), s.as_bytes());
                for mat in matches {
                    for capture in mat.captures {
                        let matched_node = capture.node;
                        let range = matched_node.range();

                        let ref_node = Symbol::new_namespace(
                            f.clone(),
                            get_shared_name(String::from(DEFAULT_NAMESPACE_REPR)),
                            range,
                        );
                        if taken.contains_key(&ref_node.id()) {
                            continue;
                        }
                        ret.push(ref_node);
                    }
                }
            }
        }

        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use crate::extractor::Extractor;
    use crate::symbol::{Symbol, SymbolKind};
    use std::sync::Arc;

    fn check_symbols(symbols: &[Symbol], expected: &[(&str, SymbolKind)]) {
        for (name, kind) in expected {
            let found = symbols.iter().any(|s| s.name.as_ref() == *name && s.kind == *kind);
            assert!(
                found,
                "Symbol '{}' with kind {:?} not found in extracted symbols",
                name, kind
            );
        }
    }

    #[test]
    fn extract_rust() {
        let code = r#"
pub fn my_function(a: i32) -> i32 {
    let b = a + 1;
    other_function(b)
}
"#;
        let symbols = Extractor::Rust.extract(Arc::new(String::from("test.rs")), &String::from(code));
        check_symbols(
            &symbols,
            &[
                ("my_function", SymbolKind::DEF),
                ("other_function", SymbolKind::REF),
            ],
        );
    }

    #[test]
    fn extract_typescript() {
        let code = r#"
export function myFunction(a: number): number {
    const b = a + 1;
    return otherFunction(b);
}
"#;
        let symbols = Extractor::TypeScript.extract(Arc::new(String::from("test.ts")), &String::from(code));
        check_symbols(
            &symbols,
            &[
                ("myFunction", SymbolKind::DEF),
                ("otherFunction", SymbolKind::REF),
            ],
        );
    }

    #[test]
    fn extract_golang() {
        let code = r#"
package main
import "fmt"
func MyFunction(a int) int {
    b := a + 1
    fmt.Println(b)
    return b
}
"#;
        let symbols = Extractor::Go.extract(Arc::new(String::from("test.go")), &String::from(code));
        check_symbols(
            &symbols,
            &[
                ("MyFunction", SymbolKind::DEF),
                ("fmt", SymbolKind::REF),
                ("Println", SymbolKind::REF),
            ],
        );
    }

    #[test]
    fn extract_python() {
        let code = r#"
def my_function(a: int) -> int:
    b = a + 1
    return other_function(b)
"#;
        let symbols = Extractor::Python.extract(Arc::new(String::from("test.py")), &String::from(code));
        check_symbols(
            &symbols,
            &[
                ("my_function", SymbolKind::DEF),
                ("other_function", SymbolKind::REF),
            ],
        );
    }

    #[test]
    fn test_python_blacklist_self() {
        let code = r#"
class MyClass:
    def method(self, arg1):
        print(self.prop)
"#;
        let symbols = Extractor::Python.extract(Arc::new("test.py".to_string()), &code.to_string());
        // "self" should be filtered by the blacklist in rule.rs
        let has_self = symbols.iter().any(|s| s.name.as_ref() == "self");
        assert!(!has_self, "Symbol 'self' should be blacklisted and ignored");

        check_symbols(
            &symbols,
            &[
                ("MyClass", SymbolKind::DEF),
                ("method", SymbolKind::DEF),
                ("print", SymbolKind::REF),
            ],
        );
    }

    #[test]
    fn test_go_regex_filter_underscore() {
        let code = r#"
package main
func main() {
    _ = "ignore me"
    val := "keep me"
}
"#;
        let symbols = Extractor::Go.extract(Arc::new("test.go".to_string()), &code.to_string());
        // "_" should be filtered by the exclude_regex in rule.rs
        let has_underscore = symbols.iter().any(|s| s.name.as_ref() == "_");
        assert!(!has_underscore, "Symbol '_' should be filtered by regex and ignored");

        check_symbols(
            &symbols,
            &[
                ("main", SymbolKind::DEF),
                ("val", SymbolKind::REF),
            ],
        );
    }

    #[test]
    fn extract_javascript() {
        let code = r#"
function myFunction(a) {
    const b = a + 1;
    return otherFunction(b);
}

class MyClass {
    constructor() {}
}
"#;
        let symbols = Extractor::JavaScript.extract(Arc::new(String::from("test.js")), &String::from(code));
        check_symbols(
            &symbols,
            &[
                ("myFunction", SymbolKind::DEF),
                ("MyClass", SymbolKind::DEF),
                ("otherFunction", SymbolKind::REF),
            ],
        );
    }

    #[test]
    fn extract_java() {
        let code = r#"
public class MyClass {
    private String myField;
    public void myMethod(int a) {
        int b = a + 1;
        otherMethod(b);
    }
    interface MyInterface {}
    enum MyEnum {}
}
"#;
        let symbols = Extractor::Java.extract(Arc::new(String::from("Test.java")), &String::from(code));
        check_symbols(
            &symbols,
            &[
                ("MyClass", SymbolKind::DEF),
                ("myField", SymbolKind::DEF),
                ("myMethod", SymbolKind::DEF),
                ("MyInterface", SymbolKind::DEF),
                ("MyEnum", SymbolKind::DEF),
                ("otherMethod", SymbolKind::REF),
            ],
        );
    }

    #[test]
    fn extract_kotlin() {
        let code = r#"
class MyClass {
    val myProp = 1
    fun myMethod(a: Int): Int {
        val b = a + 1
        return otherMethod(b)
    }
    object MyObject {}
}
"#;
        let symbols = Extractor::Kotlin.extract(Arc::new(String::from("test.kt")), &String::from(code));
        check_symbols(
            &symbols,
            &[
                ("MyClass", SymbolKind::DEF),
                ("myProp", SymbolKind::DEF),
                ("myMethod", SymbolKind::DEF),
                ("MyObject", SymbolKind::DEF),
                ("otherMethod", SymbolKind::REF),
            ],
        );
    }

    #[test]
    fn extract_swift() {
        let code = r#"
class MyClass {}
struct MyStruct {}
enum MyEnum {}
protocol MyProtocol {}
typealias MyAlias = String

func myFunc(a: Int) -> Int {
    let b = a + 1
    return otherFunc(b)
}
"#;
        let symbols = Extractor::Swift.extract(Arc::new(String::from("test.swift")), &String::from(code));
        check_symbols(
            &symbols,
            &[
                ("MyClass", SymbolKind::DEF),
                ("MyProtocol", SymbolKind::DEF),
                ("MyAlias", SymbolKind::DEF),
                ("myFunc", SymbolKind::DEF),
                ("otherFunc", SymbolKind::REF),
            ],
        );
    }

    #[test]
    fn extract_csharp() {
        let code = r#"
using System;
namespace MyApp {
    public class MyClass {
        public void MyMethod(int a) {
            int b = a + 1;
            OtherMethod(b);
        }
    }
}
"#;
        let symbols = Extractor::CSharp.extract(Arc::new(String::from("test.cs")), &String::from(code));
        check_symbols(
            &symbols,
            &[
                ("MyClass", SymbolKind::DEF),
                ("MyMethod", SymbolKind::DEF),
                ("OtherMethod", SymbolKind::REF),
            ],
        );
    }
}
