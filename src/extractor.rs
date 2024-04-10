use tree_sitter::{Parser, Query, QueryCursor, Range};
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SymbolKind {
    DEF,
    REF,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Symbol {
    pub(crate) name: String,
    pub(crate) range: Range,
    pub(crate) kind: SymbolKind,
}

impl Symbol {
    pub fn new_def(name: String, range: Range) -> Symbol {
        return Symbol {
            name,
            kind: SymbolKind::DEF,
            range,
        };
    }

    pub fn new_ref(name: String, range: Range) -> Symbol {
        return Symbol {
            name,
            kind: SymbolKind::REF,
            range,
        };
    }

    pub fn id(&self) -> String {
        return format!("{}", self.range.start_byte);
    }
}

pub enum Extractor {
    RUST,
}

impl Extractor {
    pub fn extract(&self, s: &String) -> Vec<Symbol> {
        match self {
            Extractor::RUST => {
                let mut parser = Parser::new();
                let lang = &tree_sitter_rust::language();
                parser
                    .set_language(lang)
                    .expect("Error loading Rust grammar");
                let tree = parser.parse(s, None).unwrap();

                const DEF_MATCH: &str = r#"
(function_item name: (identifier) @exported_symbol)
"#;
                const REF_MATCH: &str = "(identifier) @variable_name";
                let mut ret = Vec::new();
                {
                    let query = Query::new(lang, DEF_MATCH).unwrap();
                    let mut cursor = QueryCursor::new();
                    let matches = cursor.matches(&query, tree.root_node(), s.as_bytes());
                    for mat in matches {
                        let matched_node = mat.captures[0].node;
                        let range = matched_node.range();

                        if let Ok(str_slice) = matched_node.utf8_text(s.as_bytes()) {
                            let string = str_slice.to_string();
                            ret.push(Symbol::new_def(string, range));
                        }
                    }
                }
                {
                    let query = Query::new(lang, REF_MATCH).unwrap();
                    let mut cursor = QueryCursor::new();
                    let matches = cursor.matches(&query, tree.root_node(), s.as_bytes());
                    for mat in matches {
                        let matched_node = mat.captures[0].node;
                        let range = matched_node.range();

                        if let Ok(str_slice) = matched_node.utf8_text(s.as_bytes()) {
                            let string = str_slice.to_string();
                            ret.push(Symbol::new_ref(string, range));
                        }
                    }
                }

                return ret;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::extractor::Extractor;
    use tracing::info;

    #[test]
    fn extract_rust() {
        tracing_subscriber::fmt::init();
        let symbols = Extractor::RUST.extract(&String::from(
            r#"
pub enum Extractor {
    RUST,
}

impl Extractor {
    pub fn extract(&self, s: &String) {
        match self {
            Extractor::RUST => {
                let mut parser = Parser::new();
                let lang = &tree_sitter_rust::language();
                parser
                    .set_language(lang)
                    .expect("Error loading Rust grammar");
                let tree = parser.parse(s, None).unwrap();
                let query_str = "(function_item name: (identifier) @function)";
                let query = Query::new(lang, query_str).unwrap();

                let mut cursor = QueryCursor::new();
                let matches = cursor.matches(&query, tree.root_node(), s.as_bytes());

                for mat in matches {
                    info!("{:?}", mat);
                }
            }
        }
    }
}
"#,
        ));
        symbols.iter().for_each(|each| {
            info!("symbol: {:?}", each);
        })
    }
}
