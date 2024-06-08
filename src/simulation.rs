// We will also need to simulate txs if there are multiple txs from the same sender.
// For example imagine this scenario. best() returns 2 txs from the same sender,
// tx 1 spends all of the senders eth so now tx 2 is no longer valid.

use reth::api::ConfigureEvm;
use reth::providers::ProviderFactory;
use reth::{
    beacon_consensus::EthBeaconConsensus,
    blockchain_tree::{
        BlockchainTree, BlockchainTreeConfig, ShareableBlockchainTree, TreeExternals,
    },
    primitives::{Address, TransactionSigned},
};
use reth_db::{
    database::Database, mdbx::DatabaseArguments, models::client_version::ClientVersion,
    open_db_read_only,
};
use reth_evm_ethereum::{execute::EthExecutorProvider, EthEvmConfig};
use reth_primitives::{
    revm::env::{fill_block_env, fill_tx_env, tx_env_with_recovered},
    revm_primitives::EVMError,
    BlockNumberOrTag, ChainSpec, ChainSpecBuilder, Header, B256,
};
use reth_provider::{
    providers::{BlockchainProvider, StaticFileProvider},
    AccountReader, BlockNumReader, BlockReader, BlockReaderIdExt, BlockSource, HeaderProvider,
    ReceiptProvider, StateProvider, StateProviderFactory, TransactionsProvider,
};
use reth_revm::{
    database::StateProviderDatabase,
    db::CacheDB,
    primitives::{EnvWithHandlerCfg, ResultAndState, TransactTo, TxEnv},
    DBBox, Evm, StateBuilder, StateDBBox,
};

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub struct RethRunner<DB> {
    pub spec: Arc<ChainSpec>,
    pub provider: Arc<BlockchainProvider<DB>>,
}

pub fn simulate(txs: HashMap<Option<Address>, Vec<TransactionSigned>>) -> eyre::Result<()> {
    Ok(())
}

impl<DB> RethRunner<DB> {
    pub fn new(spec: Arc<ChainSpec>, provider: Arc<BlockchainProvider<DB>>) -> Self {
        Self { spec, provider }
    }
}

impl<DB> RethRunner<DB>
where
    DB: Database,
{
    fn run(
        &self,
        tx: &TransactionSigned,
        sender: Address,
    ) -> Result<ResultAndState, EVMError<String>> {
        let latest_block_header = self
            .provider
            .latest_header()
            .map_err(|_e| EVMError::Database(String::from("Error fetching latest sealed header")))?
            .unwrap();

        let latest_block = self
            .provider
            .block_by_hash(latest_block_header.hash())
            .map_err(|_e| EVMError::Database(String::from("Error fetching latest block")))?
            .unwrap();

        let latest_state = self
            .provider
            .state_by_block_hash(latest_block_header.hash())
            .map_err(|_| EVMError::Database(String::from("Error fetching latest state")))?;

        let state = Arc::new(StateProviderDatabase::new(latest_state));
        let db = CacheDB::new(Arc::clone(&state));
        // let mut evm = Evm::builder().with_db(db).with_cfg_env_with_handler_cfg(cfg_env_and_spec_id)
        let evm_config = EthEvmConfig::default();
        let mut evm = evm_config.evm(db);
        fill_block_env(evm.block_mut(), &self.spec, &latest_block_header, true);
        fill_tx_env(evm.tx_mut(), tx, sender);

        evm.transact()
            .map_err(|_| EVMError::Database(String::from("Error executing transaction")))
    }
}

pub struct RethRunnerBuilder {
    pub db_path: String,
}

impl RethRunnerBuilder {
    pub fn new() -> Self {
        Self {
            db_path: "./".to_string(),
        }
    }

    pub fn with_db_path(&mut self, db_path: String) -> &mut Self {
        self.db_path = db_path;
        self
    }

    pub fn build(&self) -> eyre::Result<RethRunner<Arc<reth_db::mdbx::DatabaseEnv>>> {
        let path = std::env::var("RETH_DB_PATH")?;
        let db_path = Path::new(&path);
        let db = Arc::new(open_db_read_only(
            db_path.join("db").as_path(),
            DatabaseArguments::new(ClientVersion::default()),
        )?);
        let chain_spec = Arc::new(ChainSpecBuilder::mainnet().build());
        let factory =
            ProviderFactory::new(db.clone(), chain_spec.clone(), db_path.join("static_files"))?;

        let provider = Arc::new({
            let consensus = Arc::new(EthBeaconConsensus::new(chain_spec.clone()));
            let executor = EthExecutorProvider::ethereum(chain_spec.clone());

            let tree_externals = TreeExternals::new(factory.clone(), consensus, executor);
            let tree = BlockchainTree::new(tree_externals, BlockchainTreeConfig::default(), None)?;
            let blockchain_tree = Arc::new(ShareableBlockchainTree::new(tree));

            BlockchainProvider::new(factory, blockchain_tree)?
        });

        Ok(RethRunner::new(chain_spec, provider))
    }
}
