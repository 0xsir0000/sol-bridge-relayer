/*
 * @Descripttion: js
 * @Version: 1.0
 * @Author: Yulin
 * @Date: 2024-11-20 22:10:17
 * @LastEditors: Yulin
 * @LastEditTime: 2024-11-20 22:20:50
 */
use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signer::Signer,
    transaction::Transaction,
};

pub struct TransactionBuilder {
    pub program_id: Pubkey,
    pub nonce_account: Pubkey,
}

impl TransactionBuilder {
    pub fn new(program_id: Pubkey, nonce_account: Pubkey) -> Self {
        Self {
            program_id,
            nonce_account,
        }
    }

    pub fn build_transfer_transaction(
        &self,
        amount: u64,
        nonce: u64,
        to_address: &Pubkey,
        payer: &impl Signer,
        client: &RpcClient,
    ) -> Result<Transaction> {
        let system_program = solana_sdk::system_program::id();

        let accounts = vec![
            AccountMeta::new(self.nonce_account, false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(*to_address, false),
            AccountMeta::new_readonly(system_program, false),
        ];

        let mut instruction_data = Vec::with_capacity(24);
        instruction_data.extend_from_slice(&[187, 90, 182, 138, 51, 248, 175, 98]);
        instruction_data.extend_from_slice(&amount.to_le_bytes());
        instruction_data.extend_from_slice(&nonce.to_le_bytes());

        let instruction = Instruction {
            program_id: self.program_id,
            accounts,
            data: instruction_data,
        };

        let recent_blockhash = client.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&payer.pubkey()),
            &[payer],
            recent_blockhash,
        );

        Ok(transaction)
    }
}
