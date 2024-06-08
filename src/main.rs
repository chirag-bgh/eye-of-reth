//! Run with
//!
//! ```not_rust
//! cargo run -p eye-of-reth -- node --http --ws --enable-ext
//! ```
//!
//! This installs an additional RPC method `txpoolExt_getCensoredTransactions` that can queried via [cast](https://github.com/foundry-rs/foundry)
//!
//! ```sh
//! cast rpc txpoolExt_getCensoredTransactions
//! ```

use std::{collections::HashMap, time::Instant};

use clap::Parser;
use eye_of_reth::simulation::simulate;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use reth::primitives::{IntoRecoveredTransaction, TransactionSigned};
use reth::{
    cli::{
        components::{RethNodeComponents, RethRpcComponents},
        config::RethRpcConfig,
        ext::{RethCliExt, RethNodeCommandConfig},
        Cli,
    },
    primitives::Address,
};

use reth_transaction_pool::TransactionPool;

fn main() {
    Cli::<MyRethCliExt>::parse().run().unwrap();
}

/// The type that tells the reth CLI what extensions to use
struct MyRethCliExt;

impl RethCliExt for MyRethCliExt {
    /// This tells the reth CLI to install the `txpool` rpc namespace via `RethCliTxpoolExt`
    type Node = RethCliTxpoolExt;
}

/// Our custom cli args extension that adds one flag to reth default CLI.
#[derive(Debug, Clone, Copy, Default, clap::Args)]
struct RethCliTxpoolExt {
    /// CLI flag to enable the txpool extension namespace
    #[clap(long)]
    pub enable_ext: bool,
}

impl RethNodeCommandConfig for RethCliTxpoolExt {
    // This is the entrypoint for the CLI to extend the RPC server with custom rpc namespaces.
    fn extend_rpc_modules<Conf, Reth>(
        &mut self,
        _config: &Conf,
        _components: &Reth,
        rpc_components: RethRpcComponents<'_, Reth>,
    ) -> eyre::Result<()>
    where
        Conf: RethRpcConfig,
        Reth: RethNodeComponents,
    {
        if !self.enable_ext {
            return Ok(());
        }

        // here we get the configured pool type from the CLI.
        let pool = rpc_components.registry.pool().clone();
        let ext = TxpoolExt { pool };

        // now we merge our extension namespace into all configured transports
        rpc_components.modules.merge_configured(ext.into_rpc())?;

        println!("txpool extension enabled");
        Ok(())
    }
}

/// trait interface for a custom rpc namespace: `txpool`
///
/// This defines an additional namespace where all methods are configured as trait functions.
#[cfg_attr(not(test), rpc(server, namespace = "txpoolExt"))]
#[cfg_attr(test, rpc(server, client, namespace = "txpoolExt"))]
pub trait TxpoolExtApi {
    /// Returns the number of transactions in the pool.
    #[method(name = "getCensoredTransactions")]
    fn get_censored_transactions(&self) -> RpcResult<usize>;
}
/// The type that implements the `txpool` rpc namespace trait
pub struct TxpoolExt<Pool> {
    pool: Pool,
}

const BLOCK_TIME: u64 = 12 * 2;

impl<Pool> TxpoolExtApiServer for TxpoolExt<Pool>
where
    Pool: TransactionPool + Clone + 'static,
{
    fn get_censored_transactions(&self) -> RpcResult<usize> {
        // best transactions ready to be included sorted by priority order
        let best_txs = &mut self.pool.best_transactions();

        let mut censored_txs = Vec::<TransactionSigned>::new();

        // filter txs older than 12s
        while let Some(pool_tx) = best_txs.next() {
            let now = Instant::now();
            let tx_age = now.duration_since(pool_tx.timestamp).as_secs();
            if tx_age > BLOCK_TIME {
                censored_txs.push(pool_tx.to_recovered_transaction().into_signed())
            }
        }

        let mut sender_to_txs: HashMap<Option<Address>, Vec<TransactionSigned>> = HashMap::new();
        for tx in censored_txs {
            let sender = tx.recover_signer();
            sender_to_txs
                .entry(sender)
                .or_insert_with(|| Vec::new())
                .push(tx);
        }

        // feed sender_to_txs to eth-simulator and return valid txs
        simulate(sender_to_txs);

        // let base_fee = 20;

        // let mut transactions_with_base_fee = Vec::<TransactionSigned>::new();

        // filer transactions with 0 prriority fee
        // for tx in *censored_txs {
        //     match tx.transaction {
        //         Transaction::Legacy(tx_legacy) => {}
        //         Transaction::Eip1559(tx_1559) => {
        //             if tx_1559.max_priority_fee_per_gas == 0 {
        //                 transactions_with_base_fee.push(tx)
        //             }
        //         }
        //         Transaction::Eip2930(tx_2930) => {}
        //         Transaction::Eip4844(tx_4844) => {
        //             if tx_4844.max_priority_fee_per_gas > 0 {
        //                 transactions_with_base_fee.push(tx)
        //             }
        //         }
        //     }
        // }

        Ok(0)
    }
}
