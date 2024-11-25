use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

pub struct PdaManager {
    program_id: Pubkey,
    watched_account: Pubkey,
}

impl PdaManager {
    pub fn new(program_id: Pubkey, watched_account: Pubkey) -> Self {
        Self {
            program_id,
            watched_account,
        }
    }

    pub fn find_address(&self, nonce: u64) -> (Pubkey, u8) {
        let seeds = [
            b"nonce",
            self.watched_account.as_ref(),
            &nonce.to_le_bytes(),
        ];

        Pubkey::find_program_address(&seeds, &self.program_id)
    }

    pub async fn get_transfer_info(
        &self,
        client: &RpcClient,
        pda: &Pubkey,
    ) -> Result<(u64, Pubkey)> {
        let account = client.get_account(pda)?;
        const EXPECTED_SIZE: usize = 87;

        if account.data.len() < EXPECTED_SIZE {
            return Err(anyhow::anyhow!(
                "Insufficient PDA account data length: expected {} bytes, got {} bytes",
                EXPECTED_SIZE,
                account.data.len()
            ));
        }

        let to_bytes: [u8; 32] = account.data[40..72].try_into()?;
        let to = Pubkey::from(to_bytes);

        let amount_bytes: [u8; 8] = account.data[72..80].try_into()?;
        let amount = u64::from_le_bytes(amount_bytes);

        Ok((amount, to))
    }
}
