/*
File: server.rs
Functionality: HTTP server implementation for remote graph access.
Role: Provides an Axum-based web server that exposes graph data and analysis through a RESTful API.
*/
use crate::graph::Graph;
use crate::symbol::{Symbol, SymbolKind};
use axum::extract::Query;
use axum::routing::get;
use axum::Router;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use crate::api::{FileMetadata, RelatedFileContext};
use anyhow::Context;

lazy_static::lazy_static! {
    pub static ref GRAPH_INST: Arc<RwLock<Graph>> = Arc::new(RwLock::new(Graph::empty()));
}

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
pub async fn server_main(server_conf: ServerConfig) -> anyhow::Result<()> {
    {
        let mut inst = GRAPH_INST.write();
        *inst = server_conf.graph;
    }

    let routers = create_router();

    let addr = format!("127.0.0.1:{}", server_conf.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {}", addr))?;
    
    tracing::info!("server listening on {}", addr);
    axum::serve(listener, routers)
        .await
        .context("Error running axum server")?;
    Ok(())
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
    let g = GRAPH_INST.read();
    axum::Json(g.file_metadata(params.path))
}

async fn file_relation_handler(
    Query(params): Query<FileParams>,
) -> axum::Json<Vec<RelatedFileContext>> {
    let g = GRAPH_INST.read();
    axum::Json(g.related_files(params.path))
}

async fn file_list_handler() -> axum::Json<HashSet<String>> {
    let g = GRAPH_INST.read();
    axum::Json(g.files())
}

async fn symbol_relation_handler(
    Query(params): Query<SymbolParams>,
) -> axum::Json<HashMap<String, usize>> {
    let g = GRAPH_INST.read();
    let targets: Vec<Symbol> = g
        .file_metadata(params.path)
        .symbols
        .into_iter()
        .filter(|each| {
            return each.range.start_byte == params.start_byte && each.kind != SymbolKind::NAMESPACE;
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
        // never
        _ => HashMap::new(),
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
    let g = GRAPH_INST.read();
    let ret = g.symbol_graph.symbol_mapping.get(&params.id);
    if let Some(idx) = ret {
        if let Some(symbol) = g.symbol_graph.g[*idx].get_symbol() {
            return axum::Json(Some(symbol));
        }
    }
    axum::Json(None)
}
