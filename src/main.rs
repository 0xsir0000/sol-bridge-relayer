//! Solana L1 to L2 bridge relayer implementation.
//! This module provides functionality to monitor L1 accounts and relay messages to L2.

mod config;

use crate::config::RelayerConfig;
use anyhow::{Error, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
    transaction::Transaction,
};
use std::{str::FromStr, time::Duration};
use tokio::time;

/// Represents the nonce status of an account
#[repr(C)]
#[derive(Debug)]
pub struct NonceStatus {
    pub nonce: u64,
}

impl NonceStatus {
    /// Creates a NonceStatus from raw account data bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 16 {
            return Err(Error::msg("账户数据长度不足"));
        }

        let nonce_bytes: [u8; 8] = data[8..16].try_into()?;
        let nonce = u64::from_le_bytes(nonce_bytes);

        Ok(NonceStatus { nonce })
    }
}

/// Represents different types of messages that can be relayed
#[repr(u8)]
#[derive(Debug)]
pub enum MessageType {
    Native = 0,
    Token = 1,
    NFT = 2,
}

impl MessageType {
    /// Creates a MessageType from a u8 value
    fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(MessageType::Native),
            1 => Ok(MessageType::Token),
            2 => Ok(MessageType::NFT),
            _ => Err(Error::msg("无效的 MessageType 值")),
        }
    }
}

/// Represents the information stored in a PDA account
#[repr(C)]
#[derive(Debug)]
pub struct Info {
    pub from: Pubkey,
    pub to: Pubkey,
    pub amount: u64,
    pub nonce: u64,
    pub message_type: MessageType,
}

/// Main relayer structure that handles the bridging logic
struct Relayer {
    l1_client: RpcClient,
    l2_client: RpcClient,
    watched_account: Pubkey,
    keypair: Keypair,
    last_nonce: Option<u64>,
    l1_program_id: Pubkey,
    config: RelayerConfig,
}

impl Relayer {
    /// Creates a new Relayer instance from the provided configuration
    pub fn new(config: &RelayerConfig) -> Result<Self> {
        let l1_client =
            RpcClient::new_with_commitment(config.l1_url.clone(), CommitmentConfig::confirmed());
        let l2_client =
            RpcClient::new_with_commitment(config.l2_url.clone(), CommitmentConfig::confirmed());
        let watched_account =
            Pubkey::from_str(&config.watched_account).map_err(|e| Error::msg(e.to_string()))?;
        let keypair =
            read_keypair_file(&config.wallet_path).map_err(|e| Error::msg(e.to_string()))?;
        let l1_program_id =
            Pubkey::from_str(&config.l1_program_id).map_err(|e| Error::msg(e.to_string()))?;

        Ok(Self {
            l1_client,
            l2_client,
            watched_account,
            keypair,
            last_nonce: None,
            l1_program_id,
            config: config.clone(),
        })
    }

    /// Finds the PDA address for a given nonce
    fn find_pda_address(&self, nonce: u64) -> Result<(Pubkey, u8)> {
        let seeds = [
            b"nonce",
            self.watched_account.as_ref(),
            &nonce.to_le_bytes(),
        ];

        let (pda, bump) = Pubkey::find_program_address(&seeds, &self.l1_program_id);
        Ok((pda, bump))
    }

    /// Retrieves transfer amount and destination address from a PDA account
    async fn get_transfer_amount_from_pda(&self, pda: &Pubkey) -> Result<(u64, Pubkey)> {
        let account = self.l1_client.get_account(pda)?;

        const EXPECTED_SIZE: usize = 87;

        if account.data.len() < EXPECTED_SIZE {
            return Err(Error::msg(format!(
                "PDA 账户数据长度不足: 期望 {} 字节，实际 {} 字节",
                EXPECTED_SIZE,
                account.data.len()
            )));
        }

        let to_bytes: [u8; 32] = account.data[40..72].try_into()?;
        let to = Pubkey::from(to_bytes);

        let amount_bytes: [u8; 8] = account.data[72..80].try_into()?;
        let amount = u64::from_le_bytes(amount_bytes);

        if !matches!(MessageType::from_u8(account.data[88])?, MessageType::Native) {
            return Err(Error::msg("不支持的消息类型"));
        }

        Ok((amount, to))
    }

    /// Monitors L1 account for nonce changes and initiates relay operations
    async fn monitor_and_relay(&mut self) -> Result<()> {
        loop {
            let account_data = self.l1_client.get_account_data(&self.watched_account)?;
            let nonce_status = NonceStatus::from_bytes(&account_data)?;

            if self.last_nonce != Some(nonce_status.nonce) {
                println!("检测到账户 nonce 变化，准备发送跨链交易");
                self.process_data_change(nonce_status.nonce).await?;
                self.last_nonce = Some(nonce_status.nonce);
            }

            time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Processes nonce changes by sending appropriate L2 transactions
    async fn process_data_change(&self, new_nonce: u64) -> Result<()> {
        let start_nonce = self.last_nonce.unwrap_or(0);
        for nonce in start_nonce..new_nonce {
            println!("处理 nonce 变化: {}", nonce);
            self.send_l2_transfer(nonce).await?;
        }
        Ok(())
    }

    /// Sends an L2 transfer transaction for a specific nonce
    async fn send_l2_transfer(&self, nonce: u64) -> Result<()> {
        let (pda, _bump) = self.find_pda_address(nonce)?;
        let (amount, to_address) = self.get_transfer_amount_from_pda(&pda).await?;

        println!(
            "向 L2 发送转账交易，nonce: {}，金额: {} lamports ({} SOL)，接收地址: {}",
            nonce,
            amount,
            amount as f64 / 1_000_000_000.0,
            to_address
        );

        let transaction = self.build_transaction_with_receiver(amount, &to_address)?;
        self.send_transaction_to_l2(transaction).await?;

        Ok(())
    }

    /// Builds an L2 transaction with the specified amount and receiver
    fn build_transaction_with_receiver(
        &self,
        amount: u64,
        to_address: &Pubkey,
    ) -> Result<Transaction> {
        let program_id = Pubkey::from_str(&self.config.l2_program_id)?;
        let fixed_account = Pubkey::from_str(&self.config.fixed_account)?;
        let nonce_account = Pubkey::from_str(&self.config.nonce_account)?;
        let system_program = solana_sdk::system_program::id();

        let accounts = vec![
            AccountMeta::new(nonce_account, false),
            AccountMeta::new(fixed_account, false),
            AccountMeta::new(self.keypair.pubkey(), true),
            AccountMeta::new(*to_address, false),
            AccountMeta::new_readonly(system_program, false),
        ];

        let mut instruction_data = Vec::with_capacity(16);
        instruction_data.extend_from_slice(&[187, 90, 182, 138, 51, 248, 175, 98]);
        instruction_data.extend_from_slice(&amount.to_le_bytes());

        let instruction = Instruction {
            program_id,
            accounts,
            data: instruction_data,
        };

        let recent_blockhash = self.l2_client.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&self.keypair.pubkey()),
            &[&self.keypair],
            recent_blockhash,
        );

        Ok(transaction)
    }

    /// Sends and confirms an L2 transaction
    async fn send_transaction_to_l2(&self, transaction: Transaction) -> Result<()> {
        let signature = self.l2_client.send_and_confirm_transaction(&transaction)?;
        println!("交易已发送，签名: {}", signature);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let config_path = std::env::current_dir()?.join("config.toml");
    println!("尝试加载配置文件: {}", config_path.display());

    let config = RelayerConfig::load(config_path)?;
    println!("成功加载配置文件");
    println!("钱包路径: {}", config.wallet_path);

    let mut relayer = Relayer::new(&config)?;
    relayer.monitor_and_relay().await?;

    Ok(())
}
