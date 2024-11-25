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
use solana_client::{client_error::ClientError, rpc_client::RpcClient};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
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
                Pubkey::from_str(&config.nonce_account)
                    .map_err(|e| anyhow::anyhow!("Invalid nonce account: {}", e))?,
            ),
        })
    }

    async fn monitor_and_relay(&mut self) -> Result<()> {
        loop {
            // 获取 L1 watched account 的 nonce
            let account_data = self.l1_client.get_account_data(&self.watched_account)?;
            let nonce_status = NonceStatus::from_bytes(&account_data)?;
            let l1_watched_nonce = nonce_status.nonce;

            // 获取 L2 nonce account 的状态
            let nonce_account = self
                .l2_client
                .get_account_data(&self.transaction_builder.nonce_account)?;

            let l2_nonce_status = if nonce_account.len() >= 24 {
                let l1_nonce_bytes: [u8; 8] = nonce_account[8..16].try_into()?;
                let l2_nonce_bytes: [u8; 8] = nonce_account[16..24].try_into()?;

                let l1_nonce = u64::from_le_bytes(l1_nonce_bytes);
                let l2_nonce = u64::from_le_bytes(l2_nonce_bytes);

                println!("\nNonce account state:");
                println!("- L1 nonce: {}", l1_nonce);
                println!("- L2 nonce: {}", l2_nonce);

                l1_nonce
            } else {
                return Err(anyhow::anyhow!(
                    "Invalid nonce account data length: expected at least 24 bytes, got {}",
                    nonce_account.len()
                ));
            };

            // 更新 last_nonce 为 L2 nonce account 中的值
            if self.last_nonce != Some(l2_nonce_status) {
                println!(
                    "Updating last_nonce from {} to {}",
                    self.last_nonce.unwrap_or(0),
                    l2_nonce_status
                );
                self.last_nonce = Some(l2_nonce_status);
            }

            // 如果 L1 watched account 的 nonce 大于当前处理的 nonce
            if l1_watched_nonce > l2_nonce_status {
                println!("\nProcessing nonce change...");
                println!("Current nonce from watched account: {}", l1_watched_nonce);
                println!("Current nonce from nonce account: {}", l2_nonce_status);

                // 处理从 L2 nonce 到 L1 nonce 之间的所有交易
                for nonce in l2_nonce_status..l1_watched_nonce {
                    self.send_l2_transfer(nonce).await?;
                }
            }

            time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn send_l2_transfer(&self, nonce: u64) -> Result<()> {
        println!("\nPreparing L2 transfer for nonce: {}", nonce);

        let (pda, bump) = self.pda_manager.find_address(nonce);
        println!("Found PDA: {} with bump: {}", pda, bump);

        let (amount, to_address) = self
            .pda_manager
            .get_transfer_info(&self.l1_client, &pda)
            .await?;

        println!("\nTransfer details:");
        println!(
            "- Amount: {} lamports ({} SOL)",
            amount,
            amount as f64 / 1_000_000_000.0
        );
        println!("- Recipient: {}", to_address);
        println!("- Nonce: {}", nonce);

        println!("\nBuilding transaction...");
        let transaction = self.transaction_builder.build_transfer_transaction(
            amount,
            nonce,
            &to_address,
            &self.keypair,
            &self.l2_client,
        )?;

        println!("\nTransaction details:");
        println!("- Program ID: {}", self.transaction_builder.program_id);
        println!(
            "- Nonce Account: {}",
            self.transaction_builder.nonce_account
        );
        println!("- Signer: {}", self.keypair.pubkey());

        self.send_transaction_to_l2(transaction).await
    }

    async fn send_transaction_to_l2(&self, transaction: Transaction) -> Result<()> {
        println!("\nSending transaction to L2...");
        match self.l2_client.send_and_confirm_transaction(&transaction) {
            Ok(signature) => {
                println!("Transaction successful!");
                println!("Signature: {}", signature);
                println!(
                    "Transaction link: https://explorer.solana.com/tx/{}?cluster=custom",
                    signature
                );
                Ok(())
            }
            Err(err) => {
                println!("\nTransaction failed!");
                println!("Error details:");
                println!("- Type: {:?}", err);
                if let Some(program_error) = err.get_transaction_error() {
                    println!("- Program error: {:?}", program_error);
                }
                Err(anyhow::anyhow!("L2 transaction failed: {}", err))
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("Starting relayer...");

    let config_path = std::env::current_dir()?.join("config.toml");
    println!("Loading config from: {}", config_path.display());

    let config = RelayerConfig::load(config_path)?;
    println!("Config loaded successfully");
    println!("L1 URL: {}", config.l1_url);
    println!("L2 URL: {}", config.l2_url);

    println!("Initializing relayer...");
    let mut relayer = Relayer::new(&config)?;
    println!("Relayer initialized successfully");

    println!("Starting monitoring...");
    relayer.monitor_and_relay().await?;

    Ok(())
}
