use anyhow::Result;

pub struct NonceStatus {
    pub nonce: u64,
}

impl NonceStatus {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(anyhow::anyhow!(
                "Invalid data length: expected at least 8 bytes, got {}",
                data.len()
            ));
        }

        let nonce_bytes: [u8; 8] = data[0..8].try_into()?;
        let nonce = u64::from_le_bytes(nonce_bytes);

        Ok(Self { nonce })
    }
}
