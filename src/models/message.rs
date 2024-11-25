use anyhow::{Error, Result};
use solana_sdk::pubkey::Pubkey;

#[repr(u8)]
#[derive(Debug)]
pub enum MessageType {
    Native = 0,
    Token = 1,
    NFT = 2,
}

impl MessageType {
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(MessageType::Native),
            1 => Ok(MessageType::Token),
            2 => Ok(MessageType::NFT),
            _ => Err(Error::msg("Invalid MessageType value")),
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct Info {
    pub from: Pubkey,
    pub to: Pubkey,
    pub amount: u64,
    pub nonce: u64,
    pub message_type: MessageType,
}

#[repr(C)]
#[derive(Debug)]
pub struct NonceStatus {
    pub nonce: u64,
}

impl NonceStatus {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 16 {
            return Err(Error::msg("Insufficient account data length"));
        }

        let nonce_bytes: [u8; 8] = data[8..16].try_into()?;
        let nonce = u64::from_le_bytes(nonce_bytes);

        Ok(NonceStatus { nonce })
    }
}
