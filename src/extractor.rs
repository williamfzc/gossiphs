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

        let get_node_path = |node: tree_sitter::Node, name: &str| -> String {
            let mut path = Vec::new();
            let mut curr = node.parent();
            while let Some(parent) = curr {
                let mut found_name = None;
                let mut matched_own_name = false;

                // 1. Try field names - standard and reliable
                for field_name in &["name", "identifier", "declarator", "type", "object", "operand", "receiver", "trait"] {
                    if let Some(name_node) = parent.child_by_field_name(field_name) {
                        if name_node == node {
                            matched_own_name = true;
                            break;
                        }
                        if let Ok(name_str) = name_node.utf8_text(s.as_bytes()) {
                            let n = name_str.to_string();
                            if !n.is_empty() && n != "self" && n != "this" {
                                found_name = Some(n);
                                break;
                            }
                        }
                    }
                }

                // 2. Fallback: Check for identifier-like children in naming-related nodes
                if !matched_own_name && found_name.is_none() {
                    let kind = parent.kind().to_lowercase();
                    if kind.contains("class")
                        || kind.contains("function")
                        || kind.contains("method")
                        || kind.contains("namespace")
                        || kind.contains("module")
                        || kind.contains("interface")
                        || kind.contains("struct")
                        || kind.contains("enum")
                        || kind.contains("object")
                    {
                        for i in 0..parent.child_count() {
                            let child = parent.child(i).unwrap();
                            if child == node {
                                matched_own_name = true;
                                break;
                            }
                            let child_kind = child.kind();
                            if child_kind.contains("identifier") || child_kind == "type_identifier" {
                                if let Ok(name_str) = child.utf8_text(s.as_bytes()) {
                                    let n = name_str.to_string();
                                    if !n.is_empty() && n != "self" && n != "this" {
                                        found_name = Some(n);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                if !matched_own_name {
                    if let Some(n) = found_name {
                        // Avoid redundant path elements or adding the name itself
                        let last_in_path = path.last().map(|s: &String| s.as_str()).unwrap_or(name);
                        if last_in_path != n {
                            path.push(n);
                        }
                    }
                }
                curr = parent.parent();
            }
            path.reverse();
            path.join(".")
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

                        let path = get_node_path(matched_node, &string);
                        let full_name = if path.is_empty() {
                            string
                        } else {
                            format!("{}.{}", path, string)
                        };

                        let shared_name = get_shared_name(full_name);
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

                        let path = get_node_path(matched_node, &string);
                        let full_name = if path.is_empty() {
                            string
                        } else {
                            format!("{}.{}", path, string)
                        };

                        let shared_name = get_shared_name(full_name);
                        let ref_node = Symbol::new_ref(f.clone(), shared_name, range);
                        if taken.contains_key(&ref_node.id()) {
                            continue;
                        }
                        ret.push(ref_node);
                    }
                }
            }
        }

        // dep
        {
            if !rule.dep_grammar.is_empty() {
                let query = Query::new(language, rule.dep_grammar).context("Error creating dep query")?;
                let mut cursor = QueryCursor::new();
                let matches = cursor.matches(&query, tree.root_node(), s.as_bytes());
                for mat in matches {
                    for capture in mat.captures {
                        let matched_node = capture.node;
                        let range = matched_node.range();

                        if let Ok(str_slice) = matched_node.utf8_text(s.as_bytes()) {
                            let string = str_slice.to_string();
                            // clean up quotes for interpreted strings
                            let clean_string = string.trim_matches(|c| c == '"' || c == '\'').to_string();
                            
                            let shared_name = get_shared_name(clean_string);
                            let dep_node = Symbol::new_import(f.clone(), shared_name, range);
                            ret.push(dep_node);
                        }
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
                ("my_function.other_function", SymbolKind::REF),
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
                ("myFunction.otherFunction", SymbolKind::REF),
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
                ("MyFunction.fmt", SymbolKind::REF),
                ("MyFunction.Println", SymbolKind::REF),
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
                ("my_function.other_function", SymbolKind::REF),
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
                ("MyClass.method", SymbolKind::DEF),
                ("MyClass.method.print", SymbolKind::REF),
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
                ("main.val", SymbolKind::REF),
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
                ("myFunction.otherFunction", SymbolKind::REF),
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
                ("MyClass.myField", SymbolKind::DEF),
                ("MyClass.myMethod", SymbolKind::DEF),
                ("MyClass.MyInterface", SymbolKind::DEF),
                ("MyClass.MyEnum", SymbolKind::DEF),
                ("MyClass.myMethod.otherMethod", SymbolKind::REF),
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
                ("MyClass.myProp", SymbolKind::DEF),
                ("MyClass.myMethod", SymbolKind::DEF),
                ("MyClass.MyObject", SymbolKind::DEF),
                ("MyClass.myMethod.otherMethod", SymbolKind::REF),
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
                ("myFunc.otherFunc", SymbolKind::REF),
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
                ("MyApp.MyClass", SymbolKind::DEF),
                ("MyApp.MyClass.MyMethod", SymbolKind::DEF),
                ("MyApp.MyClass.MyMethod.OtherMethod", SymbolKind::REF),
            ],
        );
    }

    #[test]
    fn test_scope_isolation_java() {
        let code = r#"
public class AuthService {
    public void login() {
        validate();
    }
    private void validate() {}
}

public class DataService {
    public void save() {
        validate();
    }
    private void validate() {}
}
"#;
        let symbols = Extractor::Java.extract(Arc::new("Test.java".to_string()), &code.to_string());
        
        // 验证 AuthService 下的 validate
        check_symbols(&symbols, &[
            ("AuthService.login", SymbolKind::DEF),
            ("AuthService.validate", SymbolKind::DEF),
            ("AuthService.login.validate", SymbolKind::REF),
        ]);

        // 验证 DataService 下的 validate
        check_symbols(&symbols, &[
            ("DataService.save", SymbolKind::DEF),
            ("DataService.validate", SymbolKind::DEF),
            ("DataService.save.validate", SymbolKind::REF),
        ]);

        // 核心验证：AuthService.login 里的 validate 引用，其 FQN 必须带有 AuthService 前缀
        // 而不是简单的 validate，从而避免误连到 DataService.validate
        let login_ref = symbols.iter().find(|s| s.name.as_ref() == "AuthService.login.validate").unwrap();
        let save_ref = symbols.iter().find(|s| s.name.as_ref() == "DataService.save.validate").unwrap();
        
        assert_ne!(login_ref.name, save_ref.name);
    }

    #[test]
    fn test_nested_scope_python() {
        let code = r#"
class Outer:
    class Inner:
        def method(self):
            helper()
    def helper(self):
        pass
"#;
        let symbols = Extractor::Python.extract(Arc::new("test.py".to_string()), &code.to_string());
        
        check_symbols(&symbols, &[
            ("Outer", SymbolKind::DEF),
            ("Outer.Inner", SymbolKind::DEF),
            ("Outer.Inner.method", SymbolKind::DEF),
            ("Outer.helper", SymbolKind::DEF),
            ("Outer.Inner.method.helper", SymbolKind::REF),
        ]);
    }
}
