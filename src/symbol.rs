use std::hash::{Hash, Hasher};
use tree_sitter::Range;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SymbolKind {
    DEF,
    REF,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Symbol {
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) range: Range,
    pub(crate) kind: SymbolKind,
}

impl Symbol {
    pub fn new_def(file: String, name: String, range: Range) -> Symbol {
        return Symbol {
            file,
            name,
            kind: SymbolKind::DEF,
            range,
        };
    }

    pub fn new_ref(file: String, name: String, range: Range) -> Symbol {
        return Symbol {
            file,
            name,
            kind: SymbolKind::REF,
            range,
        };
    }

    pub fn id(&self) -> String {
        return format!("{}{}", self.file, self.range.start_byte);
    }
}

impl Hash for Symbol {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}
