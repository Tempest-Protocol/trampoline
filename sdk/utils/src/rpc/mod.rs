use ckb_sdk::{
    traits::{
        DefaultCellCollector, DefaultCellDepResolver, DefaultHeaderDepResolver,
        DefaultTransactionDependencyProvider,
    },
    CkbRpcClient,
};
pub struct RpcInfo {
    pub node_url: String,
    pub indexer_url: String,
}

impl From<(String, String)> for RpcInfo {
    fn from((node_url, indexer_url): (String, String)) -> Self {
        Self {
            node_url,
            indexer_url,
        }
    }
}

// TrampolineProvider
// Wrapper for ckb_sdk modules:
//   * CellDepResolver
//   * HeaderDepResolver
//   * CellCollector
//   * TransactionDependencyProvider
pub struct RpcProvider {
    node_url: String,
    indexer_url: String,
}

impl RpcProvider {
    pub fn new(rpc_info: RpcInfo) -> Self {
        Self {
            node_url: rpc_info.node_url,
            indexer_url: rpc_info.indexer_url,
        }
    }

    pub fn cell_collector(&self) -> DefaultCellCollector {
        DefaultCellCollector::new(self.indexer_url.as_str(), self.node_url.as_str())
    }

    pub fn cell_dep_resolver(&self) -> DefaultCellDepResolver {
        let mut ckb_client = CkbRpcClient::new(self.node_url.as_str());
        let genesis_block = ckb_client.get_block_by_number(0.into()).unwrap().unwrap();
        DefaultCellDepResolver::from_genesis(&genesis_block.into())
            .expect("Failed creating genesis info from block")
    }

    pub fn header_dep_resolver(&self) -> DefaultHeaderDepResolver {
        DefaultHeaderDepResolver::new(self.node_url.as_str())
    }

    pub fn tx_dep_provider(&self) -> DefaultTransactionDependencyProvider {
        DefaultTransactionDependencyProvider::new(self.node_url.as_str(), 10)
    }
}

impl From<RpcInfo> for RpcProvider {
    fn from(rpc_info: RpcInfo) -> Self {
        Self::new(rpc_info)
    }
}
