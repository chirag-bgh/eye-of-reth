//! Run with
//!
//! ```not_rust
//! cargo run -p eye-of-reth -- node --http --ws --enable-ext
//! ```
//!
//! curl --location 'localhost:8545/' --header 'Content-Type: application/json' --data '{"jsonrpc":"2.0","method":"txpoolExt_getCensoredTransactions","params":[],"id":1}'
//!

use std::time::Instant;

use clap::Parser;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use reth::cli::Cli;
use reth::primitives::{IntoRecoveredTransaction, TransactionSigned};
use reth_node_ethereum::EthereumNode;
use reth_transaction_pool::TransactionPool;
use tracing::info;

fn main() {
    Cli::<RethCliTxpoolExt>::parse()
        .run(|builder, args| async move {
            let handle = builder
                .node(EthereumNode::default())
                .extend_rpc_modules(move |ctx| {
                    if !args.enable_ext {
                        return Ok(());
                    }

                    // here we get the configured pool.
                    let pool = ctx.pool().clone();

                    let ext = TxpoolExt { pool };

                    // now we merge our extension namespace into all configured transports
                    ctx.modules.merge_configured(ext.into_rpc())?;

                    println!("txpool extension enabled");

                    Ok(())
                })
                .launch()
                .await?;

            handle.wait_for_node_exit().await
        })
        .unwrap();
}

/// Our custom cli args extension that adds one flag to reth default CLI.
#[derive(Debug, Clone, Copy, Default, clap::Args)]
struct RethCliTxpoolExt {
    /// CLI flag to enable the txpool extension namespace
    #[clap(long)]
    pub enable_ext: bool,
}

/// trait interface for a custom rpc namespace: `txpool`
///
/// This defines an additional namespace where all methods are configured as trait functions.
#[cfg_attr(not(test), rpc(server, namespace = "txpoolExt"))]
#[cfg_attr(test, rpc(server, client, namespace = "txpoolExt"))]
pub trait TxpoolExtApi {
    /// Returns the number of transactions in the pool.
    #[method(name = "getCensoredTransactions")]
    fn get_censored_transactions(&self) -> RpcResult<Vec<TransactionSigned>>;
}
/// The type that implements the `txpool` rpc namespace trait
pub struct TxpoolExt<Pool> {
    pool: Pool,
}

const BLOCK_TIME: u64 = 12;

impl<Pool> TxpoolExtApiServer for TxpoolExt<Pool>
where
    Pool: TransactionPool + Clone + 'static,
{
    fn get_censored_transactions(&self) -> RpcResult<Vec<TransactionSigned>> {
        // best transactions ready to be included sorted by priority order
        let best_txs = &mut self.pool.best_transactions();
        let mut result = Vec::<TransactionSigned>::new();
        // filter txs older than 12s
        while let Some(pool_tx) = best_txs.next() {
            let now = Instant::now();
            let tx_age = now.duration_since(pool_tx.timestamp).as_secs();
            if tx_age > BLOCK_TIME {
                result.push(pool_tx.to_recovered_transaction().into_signed())
            }
        }

        info!(
            "Found {:?} transactions likely to be censored",
            result.len()
        );

        Ok(result)
    }
}
