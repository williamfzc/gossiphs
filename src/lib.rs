use cupido::collector::config::Collect;
use cupido::collector::config::{get_collector, Config};
use tracing::info;

pub fn create_cupido_graph() {
    let conf = Config::default();
    let collector = get_collector();
    let graph = collector.walk(conf);
    info!("size: {:?}", graph.size());
}
