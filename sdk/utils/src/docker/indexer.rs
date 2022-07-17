#[allow(unused_imports)]
use super::ckb::CkbNode;
#[allow(unused_imports)]
use ckb_sdk::IndexerRpcClient;
use testcontainers::{core::WaitFor, *};

const NAME: &str = "nervos/ckb-indexer";
const TAG: &str = "latest";

#[derive(Debug, Default)]
pub struct CkbIndexer;

#[derive(Debug, Default, Clone)]
pub struct CkbIndexerArgs;

impl ImageArgs for CkbIndexerArgs {
    fn into_iterator(self) -> Box<dyn Iterator<Item = String>> {
        Box::new(
            vec![
                "-s".to_string(),
                "data".to_string(),
                "-c".to_string(),
                "tcp://ckb-test-node:8114".to_string(),
            ]
            .into_iter(),
        )
    }
}

impl Image for CkbIndexer {
    type Args = CkbIndexerArgs;

    fn name(&self) -> String {
        NAME.to_owned()
    }

    fn tag(&self) -> String {
        TAG.to_owned()
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::Duration {
            length: std::time::Duration::from_secs(5),
        }]
    }
}

#[test]
fn indexer_image() {
    let _ = pretty_env_logger::try_init();
    let docker = clients::Cli::default();

    // Setup runnables
    let node_runnable = RunnableImage::from(CkbNode)
        .with_container_name("ckb-test-node")
        .with_network("test");

    // let indexer_image = images::generic::GenericImage::new("nervos/ckb-indexer", "latest")
    //     .with_entrypoint("/bin/ckb-indexer -s data -c http://ckb-test-node:8114")
    //     .with_wait_for(WaitFor::seconds(3));

    let indexer_runnable = RunnableImage::from(CkbIndexer)
        .with_container_name("ckb-test-indexer")
        .with_network("test");

    let _node = docker.run(node_runnable);
    let indexer = docker.run(indexer_runnable);

    let indexer_port = indexer.get_host_port_ipv4(8116);

    let mut client = IndexerRpcClient::new(&format!("http://127.0.0.1:{}", indexer_port));
    let tip = client.get_tip().unwrap();

    assert!(tip.is_some());
}
