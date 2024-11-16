use crate::graph::{Graph};
use crate::symbol::{Symbol, SymbolKind};
use axum::extract::Query;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use crate::api::{FileMetadata, RelatedFileContext};

lazy_static::lazy_static! {
    pub static ref GRAPH_INST: Arc<RwLock<Graph>> = Arc::new(RwLock::new(Graph::empty()));
}

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
pub async fn server_main(server_conf: ServerConfig) {
    *GRAPH_INST.write().unwrap() = server_conf.graph;

    let routers = create_router();

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", server_conf.port))
        .await
        .unwrap();
    axum::serve(listener, routers).await.unwrap();
}

pub fn create_router() -> Router {
    Router::new()
        .nest(
            "/file",
            Router::new()
                .route("/metadata", get(file_metadata_handler))
                .route("/relation", get(file_relation_handler))
                .route("/list", get(file_list_handler)),
        )
        .nest(
            "/symbol",
            Router::new()
                .route("/relation", get(symbol_relation_handler))
                .route("/metadata", get(symbol_metadata_handler)),
        )
        .route("/", get(root_handler))
}

pub struct ServerConfig {
    pub port: u16,
    pub graph: Graph,
}

impl ServerConfig {
    pub fn new(g: Graph) -> ServerConfig {
        ServerConfig {
            port: 9411,
            graph: g,
        }
    }
}

async fn root_handler() -> axum::Json<Desc> {
    axum::Json(Desc {
        version: VERSION.to_string(),
    })
}

#[derive(Deserialize, Serialize, Debug)]
struct Desc {
    version: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct FileParams {
    pub path: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct SymbolParams {
    pub path: String,
    pub start_byte: usize,
}

#[derive(Deserialize, Serialize, Debug)]
struct SymbolIdParams {
    pub id: String,
}

async fn file_metadata_handler(Query(params): Query<FileParams>) -> axum::Json<FileMetadata> {
    let g = GRAPH_INST.read().unwrap();
    axum::Json(g.file_metadata(params.path))
}

async fn file_relation_handler(
    Query(params): Query<FileParams>,
) -> axum::Json<Vec<RelatedFileContext>> {
    let g = GRAPH_INST.read().unwrap();
    axum::Json(g.related_files(params.path))
}

async fn file_list_handler() -> axum::Json<HashSet<String>> {
    let g = GRAPH_INST.read().unwrap();
    axum::Json(g.files())
}

async fn symbol_relation_handler(
    Query(params): Query<SymbolParams>,
) -> axum::Json<HashMap<String, usize>> {
    let g = GRAPH_INST.read().unwrap();
    let targets: Vec<Symbol> = g
        .file_metadata(params.path)
        .symbols
        .into_iter()
        .filter(|each| {
            return each.range.start_byte == params.start_byte;
        })
        .collect();
    if targets.len() == 0 {
        return axum::Json(HashMap::new());
    }
    // only one
    let target = &targets[0];
    let symbol_map = match target.kind {
        SymbolKind::DEF => g.symbol_graph.list_references_by_definition(&target.id()),
        SymbolKind::REF => g.symbol_graph.list_definitions_by_reference(&target.id()),
    };
    let str_symbol_map: HashMap<String, usize> = symbol_map
        .into_iter()
        .map(|(key, value)| {
            return (key.id(), value);
        })
        .collect();
    axum::Json(str_symbol_map)
}

async fn symbol_metadata_handler(
    Query(params): Query<SymbolIdParams>,
) -> axum::Json<Option<Symbol>> {
    let g = GRAPH_INST.read().unwrap();
    let ret = g.symbol_graph.symbol_mapping.get(&params.id);
    if ret.is_none() {
        return axum::Json(None);
    }

    axum::Json(Option::from(
        g.symbol_graph.g[*ret.unwrap()].get_symbol().unwrap(),
    ))
}
