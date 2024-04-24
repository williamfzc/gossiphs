use crate::graph::{FileMetadata, Graph, RelatedFileContext};
use axum::extract::Query;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

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
    return Router::new()
        .nest(
            "/file",
            Router::new()
                .route("/metadata", get(file_metadata_handler))
                .route("/relation", get(file_relation_handler))
                .route("/list", get(file_list_handler)),
        )
        .route("/", get(root_handler));
}

pub struct ServerConfig {
    pub port: u16,
    pub graph: Graph,
}

impl ServerConfig {
    pub fn new(g: Graph) -> ServerConfig {
        return ServerConfig {
            port: 9411,
            graph: g,
        };
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

async fn file_metadata_handler(Query(params): Query<FileParams>) -> axum::Json<FileMetadata> {
    let g = GRAPH_INST.read().unwrap();
    return axum::Json(g.file_metadata(&params.path));
}

async fn file_relation_handler(
    Query(params): Query<FileParams>,
) -> axum::Json<Vec<RelatedFileContext>> {
    let g = GRAPH_INST.read().unwrap();
    return axum::Json(g.related_files(&params.path));
}

async fn file_list_handler() -> axum::Json<HashSet<String>> {
    let g = GRAPH_INST.read().unwrap();
    return axum::Json(g.files());
}
