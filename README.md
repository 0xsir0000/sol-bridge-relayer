# Solana Bridge Relayer

A relay implementation for cross-chain message passing from Solana L1 to L2.

## Project Structure

sol-bridge-relayer/ 

## Dependencies

Main dependencies:
- solana-sdk: ~1.14.0
- solana-client: ~1.14.0
- solana-program: ~1.14.0
- tokio: 1.28 (Async runtime)
- anyhow: Error handling
- serde: Serialization support
- config: Configuration file handling

## Implementation Details

### Account Data Structures

1. NonceStatus: Store and track nonce values
2. MessageType: Supported message types (Native/Token/NFT)
3. Info: Cross-chain message information stored in PDA accounts

### L2 Transaction Building

- Build instruction data using correct Anchor discriminator
- Construct transactions following Anchor program account ordering

## Important Notes

1. Ensure all addresses and paths in the configuration file are correct
2. Wallet must have sufficient SOL for transaction fees
3. Monitored accounts must have the correct data structure
4. L2 program must be correctly deployed and accessible

## Development Roadmap

- [ ] Add more error handling and retry mechanisms
- [ ] Support more types of cross-chain messages
- [ ] Add monitoring and logging system
- [ ] Optimize performance and resource usage