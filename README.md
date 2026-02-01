# Sovereign Citadel Protocol (SCP)

> **A Bitcoin-native liquidity and prediction layer secured by Discreet Log Contracts (DLCs) and AI Agents.**

![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)
![Build Status](https://img.shields.io/badge/build-passing-brightgreen)

## Overview

The Sovereign Citadel Protocol (SCP) is a decentralized system that enables trustless financial contracts on Bitcoin. It combines three core pillars:

1.  **Discreet Log Contracts (DLCs)**: Cryptographically secure, oracle-based execution of financial terms without on-chain footprint for intermediate states.
2.  **AI Sentinel (SCP-Agent)**: An autonomous risk management system that monitors market sentiment and on-chain metrics to dynamically adjust protocol parameters (spreads, circuit breakers).
3.  **Liquidity Vault (SCP-Vault)**: A delta-neutral liquidity pool that facilitates DLC counterparty matching, secured by the protocol's inventory.

## Architecture

The system is composed of several modular crates:

- **`scp-core`**: Shared types, cryptographic primitives (`Satoshi`, `ContractId`), and error handling.
- **`scp-dlc`**: The core DLC engine handling state transitions (Offer -> Accept -> Sign -> Settle) and oracle attestations.
- **`scp-agent`**: The "brain" of the protocol. Connects to the vault via async channels to broadcast risk signals.
- **`scp-vault`**: Manages the liquidity pool, handles user deposits/withdrawals, and executes hedging strategies.
- **`scp-economics`**: Manages the $CTDL governance token, staking rewards, and fee distribution.
- **`scp-signer`**: Isolated key management module (Keys never touch the Agent).
- **`scp-cli`**: The command-line interface for interacting with the protocol.

### Project Structure

```
crates/
├── scp-core/       # Shared types and cryptographic primitives
├── scp-dlc/        # DLC state machine and contract logic
├── scp-agent/      # AI Sentinel for risk management (Sentiment Analysis)
├── scp-vault/      # Liquidity Vault and Delta Neutral strategies
├── scp-economics/  # $CTDL token, staking, and fee distribution
├── scp-signer/     # Isolated key management and signing policy
└── scp-chain/      # Bitcoin network interface (BDK wrapper)
```

## Installation

Ensure you have Rust and Cargo installed.

```bash
git clone https://github.com/your-repo/sovereign-citadel.git
cd sovereign-citadel
cargo build --release
```

To install the CLI globally:

```bash
cargo install --path cli
```

## Usage

### 1. Initialization

Initialize the protocol data directory (defaults to `~/.sovereign-citadel` or similar on your OS).

```bash
scp init --network signet
```

### 2. Running a Node

Start the full node, which spins up the Vault and Agent services.

```bash
scp run
```

### 3. Token Operations ($CTDL)

Manage your governance tokens.

```bash
# Check status and get your generated Public Key
scp token status

# Mint tokens (Testnet/Signet only)
scp token mint --to <YOUR_PUBLIC_KEY> --amount 1000

# Stake tokens to earn protocol fees
scp token stake --amount 500
```

### 4. DLC Operations

Interact with Discreet Log Contracts.

```bash
# List active contracts
scp dlc list

# View contract details
scp dlc show <CONTRACT_ID>
```

## Development & Testing

Run the full suite of unit and integration tests.

```bash
cargo test --workspace
```

### The "God Test"

We have a comprehensive end-to-end integration test that simulates the entire lifecycle of the protocol, from Agent sentiment shifts to DLC settlement.

```bash
cargo test --test full_cycle_test
```

## License

MIT License. See [LICENSE](LICENSE) for details.
