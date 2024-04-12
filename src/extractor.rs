use crate::rule::get_rule;
use crate::symbol::Symbol;
use std::collections::HashMap;
use tree_sitter::{Language, Parser, Query, QueryCursor};

pub enum Extractor {
    Rust,
    TypeScript,
}

impl Extractor {
    pub fn extract(&self, s: &String) -> Vec<Symbol> {
        return match self {
            Extractor::Rust => {
                let lang = &tree_sitter_rust::language();
                self._extract(s, lang)
            }
            Extractor::TypeScript => {
                let lang = &tree_sitter_typescript::language_typescript();
                self._extract(s, lang)
            }
        }
    }

    fn _extract(&self, s: &String, language: &Language) -> Vec<Symbol> {
        let mut parser = Parser::new();
        parser
            .set_language(*language)
            .expect("Error loading grammar");
        let tree = parser.parse(s, None).unwrap();

        let rule = get_rule(&self);
        let mut ret = Vec::new();
        let mut taken = HashMap::new();

        // defs
        {
            let query = Query::new(*language, rule.export_grammar).unwrap();
            let mut cursor = QueryCursor::new();
            let matches = cursor.matches(&query, tree.root_node(), s.as_bytes());
            for mat in matches {
                let matched_node = mat.captures[0].node;
                let range = matched_node.range();

                if let Ok(str_slice) = matched_node.utf8_text(s.as_bytes()) {
                    let string = str_slice.to_string();
                    let def_node = Symbol::new_def(string, range);
                    taken.insert(def_node.id(), ());
                    ret.push(def_node);
                }
            }
        }

        // refs
        {
            let query = Query::new(*language, rule.import_grammar).unwrap();
            let mut cursor = QueryCursor::new();
            let matches = cursor.matches(&query, tree.root_node(), s.as_bytes());
            for mat in matches {
                let matched_node = mat.captures[0].node;
                let range = matched_node.range();

                if let Ok(str_slice) = matched_node.utf8_text(s.as_bytes()) {
                    let string = str_slice.to_string();
                    let ref_node = Symbol::new_ref(string, range);
                    if taken.contains_key(&ref_node.id()) {
                        continue;
                    }
                    ret.push(ref_node);
                }
            }
        }

        return ret;
    }
}

#[cfg(test)]
mod tests {
    use crate::extractor::Extractor;
    use tracing::info;

    #[test]
    fn extract_rust() {
        tracing_subscriber::fmt::init();
        let symbols = Extractor::Rust.extract(&String::from(
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

    #[test]
    fn extract_typescript() {
        tracing_subscriber::fmt::init();
        let symbols = Extractor::TypeScript.extract(&String::from(
            r#"
import { store } from 'docx-deps';

import { toggleShowCommentNumbers } from '$common/redux/actions';

export interface ClickEvent {
  index: number;
  commentIds: string[];
}

function abc() {};

class NumbersManager {
  private hideNumberTimer: number | null = null;

  destroy() {
    this.clearHideNumberTimer();
  }

  temporaryHideNumbers() {
    this.clearHideNumberTimer();
    store.dispatch(toggleShowCommentNumbers(false));
  }

  showNumbers() {
    this.clearHideNumberTimer();

    this.hideNumberTimer = window.setTimeout(() => {
      store.dispatch(toggleShowCommentNumbers(true));
    }, 600);
  }

  private clearHideNumberTimer() {
    this.hideNumberTimer && window.clearTimeout(this.hideNumberTimer);
  }
}

export default NumbersManager;
            ""#));
        symbols.iter().for_each(|each| {
            info!("symbol: {:?}", each);
        })
    }
}
