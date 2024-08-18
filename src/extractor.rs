use crate::rule::get_rule;
use crate::symbol::Symbol;
use std::collections::HashMap;
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
}

impl Extractor {
    pub fn extract(&self, f: &String, s: &String) -> Vec<Symbol> {
        match self {
            Extractor::Rust => {
                let lang = &tree_sitter_rust::language();
                self._extract(f, s, lang)
            }
            Extractor::TypeScript => {
                let lang = &tree_sitter_typescript::language_typescript();
                self._extract(f, s, lang)
            }
            Extractor::Go => {
                let lang = &tree_sitter_go::language();
                self._extract(f, s, lang)
                    .into_iter()
                    .filter(|each| {
                        return each.name != "_";
                    })
                    .collect()
            }
            Extractor::Python => {
                let lang = &tree_sitter_python::language();
                self._extract(f, s, lang)
            }
            Extractor::JavaScript => {
                let lang = &tree_sitter_javascript::language();
                self._extract(f, s, lang)
            }
            Extractor::Java => {
                let lang = &tree_sitter_javascript::language();
                self._extract(f, s, lang)
            }
            Extractor::Kotlin => {
                let lang = &tree_sitter_kotlin::language();
                self._extract(f, s, lang)
            }
            Extractor::Swift => {
                let lang = &tree_sitter_swift::language();
                self._extract(f, s, lang)
            }
        }
    }

    fn _extract(&self, f: &String, s: &String, language: &Language) -> Vec<Symbol> {
        let mut parser = Parser::new();
        parser
            .set_language(language)
            .expect("Error loading grammar");
        let tree = parser.parse(s, None).unwrap();

        let rule = get_rule(&self);
        let mut ret = Vec::new();
        let mut taken = HashMap::new();

        // defs
        {
            let query = Query::new(language, rule.export_grammar).unwrap();
            let mut cursor = QueryCursor::new();
            let matches = cursor.matches(&query, tree.root_node(), s.as_bytes());
            for mat in matches {
                let matched_node = mat.captures[0].node;
                let range = matched_node.range();

                if let Ok(str_slice) = matched_node.utf8_text(s.as_bytes()) {
                    let string = str_slice.to_string();
                    let def_node = Symbol::new_def(f.clone(), string, range);
                    taken.insert(def_node.id(), ());
                    ret.push(def_node);
                }
            }
        }

        // refs
        {
            let query = Query::new(language, rule.import_grammar).unwrap();
            let mut cursor = QueryCursor::new();
            let matches = cursor.matches(&query, tree.root_node(), s.as_bytes());
            for mat in matches {
                let matched_node = mat.captures[0].node;
                let range = matched_node.range();

                if let Ok(str_slice) = matched_node.utf8_text(s.as_bytes()) {
                    let string = str_slice.to_string();
                    let ref_node = Symbol::new_ref(f.clone(), string, range);
                    if taken.contains_key(&ref_node.id()) {
                        continue;
                    }
                    ret.push(ref_node);
                }
            }
        }

        ret
    }
}

#[cfg(test)]
mod tests {
    use crate::extractor::Extractor;
    use std::fs;
    use tracing::info;

    #[test]
    fn extract_rust() {
        let symbols = Extractor::Rust.extract(
            &String::from("abc"),
            &String::from(
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
            ),
        );
        symbols.iter().for_each(|each| {
            info!("symbol: {:?}", each);
        })
    }

    #[test]
    fn extract_typescript() {
        let symbols = Extractor::TypeScript.extract(
            &String::from("abc"),
            &String::from(
                r#"
import { store } from 'docx-deps';

import { toggleShowCommentNumbers } from '$common/redux/actions';

export type DocVerseDeps = DocVerseDepsImpl;
export { loadBlockitUIComponent } from 'docx-deps';
export const aaaaa = "cbde";

export interface ClickEvent {
  index: number;
  commentIds: string[];
}

export const loadUrlWebSDKResource = async () => {
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
            ""#,
            ),
        );
        symbols.iter().for_each(|each| {
            info!("symbol: {:?}", each);
        })
    }

    #[test]
    fn extract_golang() {
        let symbols = Extractor::Go.extract(
            &String::from("abc"),
            &String::from(
                r#"
package abc

type Parser struct {
	*Headless
	engine *sitter.Parser
}

func NormalFunc(lang *sitter.Language) string {
	return "hello"
}

func (*Parser) NormalMethod(lang *sitter.Language) string {
	return "hi"
}

func Abcd[T DataType](result *BaseFileResult[T]) []T {
	return nil
}

func injectV1Group(v1group *gin.RouterGroup) {
	// scope
	scopeGroup := v1group.Group("/")
}

const a = "1"
var b = "2"
type c = d
            "#,
            ),
        );
        symbols.iter().for_each(|each| {
            info!("symbol: {:?}", each);
        })
    }

    #[test]
    #[ignore]
    fn extract_typescript_file() {
        // for testing extract rules
        tracing_subscriber::fmt::init();
        let file_path = "";
        let file_content = &fs::read_to_string(file_path).unwrap_or_default();
        let symbols = Extractor::TypeScript.extract(&String::from(file_path), file_content);
        symbols.iter().for_each(|each| {
            info!("symbol: {:?} {:?}", each.name, each.kind);
        })
    }

    #[test]
    fn extract_python() {
        let symbols = Extractor::Python.extract(
            &String::from("abc"),
            &String::from(
                r#"
def normal_fff(self, env_config: EnvConfig):
    pass

class BaseStep(object):
    def apply(self, env_config: EnvConfig, result: ResultContext):
        raise NotImplementedError

    def name(self) -> str:
        raise NotImplementedError

    def config_name(self) -> str:
        return self.name().replace("-", "_")

    def get_mod_config(self, env_config: EnvConfig):
        return getattr(
            env_config._repo_config.modules,
            self.config_name(),
        )

    def enabled(self, env_config: EnvConfig) -> bool:
        mod_config = self.get_mod_config(env_config)
        return mod_config.enabled
            "#,
            ),
        );
        symbols.iter().for_each(|each| {
            info!("symbol: {:?}", each);
        })
    }

    #[test]
    fn extract_javascript() {
        let symbols = Extractor::JavaScript.extract(
            &String::from("abc"),
            &String::from(
                r#"
import React from 'react';
import { Component } from 'react';
import { SomeDefaultExport } from './some-module';
import * as SomeNamespace from './some-namespace';
import { namedFunction, namedClass } from './some-library';

export default function exampleFunction() {
    console.log('This is an example function.');
}

export function namedFunction() {
    console.log('This is a named function.');
}

export class namedClass {
    constructor() {
        console.log('This is a named class.');
    }
}

const exportsObject = {
    anotherFunction: function() {
        console.log('This is another function.');
    },
    anotherClass: class {
        constructor() {
            console.log('This is another class.');
        }
    }
};

export { exportsObject };
            "#,
            ),
        );
        symbols.iter().for_each(|each| {
            info!("symbol: {:?}", each);
        })
    }

    #[test]
    fn extract_java() {
        let symbols = Extractor::Java.extract(
            &String::from("abc"),
            &String::from(
                r#"
package example;
import com.google.common.util.concurrent.Futures;

public class Example {
	public static void hello() {
		System.out.println(Futures.immediateCancelledFuture());
	}
}
            "#,
            ),
        );
        symbols.iter().for_each(|each| {
            info!("symbol: {:?}", each);
        })
    }

    #[test]
    fn extract_kt() {
        let symbols = Extractor::Kotlin.extract(
            &String::from("abc"),
            &String::from(
                r#"
package com.google.samples.apps.nowinandroid.core.data

import android.util.Log
import com.google.samples.apps.nowinandroid.core.datastore.ChangeListVersions
import com.google.samples.apps.nowinandroid.core.network.model.NetworkChangeList
import kotlin.coroutines.cancellation.CancellationException

interface Synchronizer {
    suspend fun getChangeListVersions(): ChangeListVersions
    suspend fun updateChangeListVersions(update: ChangeListVersions.() -> ChangeListVersions)
    suspend fun Syncable.sync() = this@sync.syncWith(this@Synchronizer)
}

interface Syncable {
    suspend fun syncWith(synchronizer: Synchronizer): Boolean
}

private suspend fun <T> suspendRunCatching(block: suspend () -> T): Result<T> = try {
    Result.success(block())
} catch (cancellationException: CancellationException) {
    throw cancellationException
} catch (exception: Exception) {
    Log.i(
        "suspendRunCatching",
        "Failed to evaluate a suspendRunCatchingBlock. Returning failure Result",
        exception,
    )
    Result.failure(exception)
}
            "#,
            ),
        );
        symbols.iter().for_each(|each| {
            info!("symbol: {:?}", each);
        })
    }

    #[test]
    fn extract_swift() {
        let symbols = Extractor::Swift.extract(
            &String::from("abc"),
            &String::from(
                r#"
import UIKit
import SwiftyJSON

@UIApplicationMain
class AppDelegate: UIResponder, UIApplicationDelegate {

    var window: UIWindow?

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {

        let navigationController = self.window?.rootViewController as! UINavigationController
        let viewController = navigationController.topViewController as! ViewController

        if let file = Bundle.main.path(forResource: "SwiftyJSONTests", ofType: "json") {
            do {
                let data = try Data(contentsOf: URL(fileURLWithPath: file))
                let json = try JSON(data: data)
                viewController.json = json
            } catch {
                viewController.json = JSON.null
            }
        } else {
            viewController.json = JSON.null
        }

        return true
    }
}
            "#,
            ),
        );
        symbols.iter().for_each(|each| {
            info!("symbol: {:?}", each);
        })
    }
}
