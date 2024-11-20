//! Solana L1 to L2 bridge relayer implementation.
//! This module provides functionality to monitor L1 accounts and relay messages to L2.

mod config;
mod models;
mod pda;
mod transaction;

use crate::{
    config::RelayerConfig, models::message::NonceStatus, pda::PdaManager,
    transaction::TransactionBuilder,
};

use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair},
    transaction::Transaction,
};
use std::{str::FromStr, time::Duration};
use tokio::time;

struct Relayer {
    l1_client: RpcClient,
    l2_client: RpcClient,
    watched_account: Pubkey,
    keypair: Keypair,
    last_nonce: Option<u64>,
    pda_manager: PdaManager,
    transaction_builder: TransactionBuilder,
}

impl Relayer {
    pub fn new(config: &RelayerConfig) -> Result<Self> {
        let l1_client =
            RpcClient::new_with_commitment(config.l1_url.clone(), CommitmentConfig::confirmed());
        let l2_client =
            RpcClient::new_with_commitment(config.l2_url.clone(), CommitmentConfig::confirmed());
        let watched_account = Pubkey::from_str(&config.watched_account)
            .map_err(|e| anyhow::anyhow!("Invalid watched account: {}", e))?;
        let keypair = read_keypair_file(&config.wallet_path)
            .map_err(|e| anyhow::anyhow!("Failed to read keypair file: {}", e))?;
        let l1_program_id = Pubkey::from_str(&config.l1_program_id)
            .map_err(|e| anyhow::anyhow!("Invalid L1 program ID: {}", e))?;
        let l2_program_id = Pubkey::from_str(&config.l2_program_id)
            .map_err(|e| anyhow::anyhow!("Invalid L2 program ID: {}", e))?;

        Ok(Self {
            l1_client,
            l2_client,
            watched_account,
            keypair,
            last_nonce: None,
            pda_manager: PdaManager::new(l1_program_id, watched_account),
            transaction_builder: TransactionBuilder::new(
                l2_program_id,
                Pubkey::from_str(&config.fixed_account)
                    .map_err(|e| anyhow::anyhow!("Invalid fixed account: {}", e))?,
                Pubkey::from_str(&config.nonce_account)
                    .map_err(|e| anyhow::anyhow!("Invalid nonce account: {}", e))?,
            ),
        })
    }

    async fn monitor_and_relay(&mut self) -> Result<()> {
        loop {
            let account_data = self.l1_client.get_account_data(&self.watched_account)?;
            let nonce_status = NonceStatus::from_bytes(&account_data)?;

            if self.last_nonce != Some(nonce_status.nonce) {
                self.process_data_change(nonce_status.nonce).await?;
                self.last_nonce = Some(nonce_status.nonce);
            }

            time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn process_data_change(&self, new_nonce: u64) -> Result<()> {
        let start_nonce = self.last_nonce.unwrap_or(0);
        for nonce in start_nonce..new_nonce {
            self.send_l2_transfer(nonce).await?;
        }
        Ok(())
    }

    async fn send_l2_transfer(&self, nonce: u64) -> Result<()> {
        let (pda, _bump) = self.pda_manager.find_address(nonce);
        let (amount, to_address) = self
            .pda_manager
            .get_transfer_info(&self.l1_client, &pda)
            .await?;

        let transaction = self.transaction_builder.build_transfer_transaction(
            amount,
            &to_address,
            &self.keypair,
            &self.l2_client,
        )?;

        self.send_transaction_to_l2(transaction).await
    }

    async fn send_transaction_to_l2(&self, transaction: Transaction) -> Result<()> {
        match self.l2_client.send_and_confirm_transaction(&transaction) {
            Ok(_) => Ok(()),
            Err(err) => Err(anyhow::anyhow!("L2 transaction failed: {}", err)),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let config_path = std::env::current_dir()?.join("config.toml");
    let config = RelayerConfig::load(config_path)?;
    let mut relayer = Relayer::new(&config)?;
    relayer.monitor_and_relay().await?;
    Ok(())
}
