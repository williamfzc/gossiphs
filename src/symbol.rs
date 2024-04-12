use tree_sitter::Range;

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
