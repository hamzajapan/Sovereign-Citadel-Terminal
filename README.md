# Sovereign Citadel Protocol (SCP)

> **A decentralized, Bitcoin-native financial system secured by Discreet Log Contracts (DLCs) and managed by AR-driven AI Agents.**

![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)
![Build Status](https://img.shields.io/badge/build-passing-brightgreen)

## 1. Core Philosophy: Why SCP?
Most centralized exchanges (CEXs) sell you "paper Bitcoin"—IOUs that can vanish if the platform fails. SCP solves this via a simple equation:
**Real Financial Yield + Full Private Key Sovereignty.**

SCP allows users to provide liquidity and speculate on Bitcoin’s price without their coins ever leaving their control, utilizing **Discreet Log Contracts (DLCs)** to ensure trustless settlement directly on the Bitcoin blockchain.

---

## 2. Technical Architecture
The system operates as a modular ecosystem composed of three primary functional layers:

### A. The Brain: AI Sentinel (SCP-Agent) 🧠
An autonomous, 24/7 risk management engine that acts as the protocol's tactical commander.
- **Sentiment Analysis**: Monitors global "Market Sentiment" by analyzing real-time news feeds and social data.
- **Dynamic Risk Response**: 
    - **Adaptive Spreads**: In cases of "Extreme Fear," it signals the Vault to widen spreads, protecting liquidity providers from volatile market flow.
    - **Circuit Breaker**: Automatically pauses trading if it detects market manipulation or sudden liquidity collapses.
- **Zero-Trust Security**: The Agent never touches private keys. It issues strategic signals but has no authority to move funds.

### B. The Body: Liquidity Vault (SCP-Vault) 🏦
The financial backend that manages the protocol's balance sheet and executes liquidity strategies.
- **Delta-Neutral Strategy**: Automatically hedges protocol positions to ensure USD value stability while capturing yield from trading fees.
- **Counterparty Matching**: Receives DLC requests and coordinates with the **Signer** to approve them based on strict, predefined safety policies.

### C. The Heart: DLC Engine (SCP-DLC) 🔐
The core innovation enabling non-custodial contracts on Bitcoin.
- **Cryptographic Foundation**: Utilizing **Schnorr Signatures** and **Adaptor Signatures** (via `secp256k1-zkp`).
- **Trustless Execution**: Users and the platform exchange "partial signatures" that only become valid when an independent Oracle publishes the price attestation.
- **Privacy & Scalability**: Settlement occurs off-chain until completion, and when broadcast, it appears on the blockchain as a standard Taproot transaction.

---

## 3. The Economic Flywheel 💸
The $CTDL token is designed for sustainable **Real Yield**, moving away from inflationary reward models.
1.  **Traders**: Pay trading fees in satoshis to open hedging or speculative DLC positions.
2.  **Protocol**: Aggregates all collected fees at the Treasury level.
3.  **Stakers**: Lock $CTDL tokens in the Governance Contract.
4.  **Fee Distribution**: A specialized **MasterChef** algorithm distributes satoshis to stakers with $O(1)$ efficiency using the formula:
    `Reward = (UserShare * AccRewardPerShare) - RewardDebt`

---

## 4. System Flow: The "God Cycle"
The following describes the full lifecycle of a protocol operation:
1.  **Analysis**: The Agent monitors news → Detects "Market Fear" → Broadcasts a `WidenSpread` signal.
2.  **Response**: The Vault receives the signal → Increases trading fees from 1% to 3% to offset elevated risk.
3.  **Request**: A user (Alice) requests a "Long" contract for 0.1 BTC via the **Tauri UI**.
4.  **Matching**: The Vault approves the request and reserves 0.1 BTC of protocol liquidity.
5.  **Hardening**: A DLC is constructed and signed by both parties off-chain.
6.  **Settlement**: Upon contract expiry, the Oracle signs the price → The winning transaction is broadcast → Profits are distributed trustlessly.

---

## 5. Technical Stack
Built for **Antifragility** and high-performance safety:
- **Language**: [Rust](https://www.rust-lang.org/) (Memory safety and performance).
- **Frontend**: [Tauri v2](https://v2.tauri.app/) + React + TypeScript (Secure, cross-platform desktop shell).
- **Cryptography**: `secp256k1-zkp` (The gold standard for Bitcoin-native contracts).
- **Blockchain**: [BDK](https://bitcoindevkit.org/) / [LDK](https://lightningdevkit.org/) (Professional Bitcoin integration).

---

## Installation & Development

### Prerequisites
- Rust (Latest Stable)
- Node.js (v20+)
- npm

### Build
```bash
# Clone the repository
git clone https://github.com/hamzajapan/Sovereign-Citadel-Terminal.git

# Build the workspace
cargo build --workspace

# Start the Desktop Client (Dev Mode)
cd crates/scp-ui
npm install
npm run tauri dev
```

### Testing
Run the comprehensive integration suite (The "God Test"):
```bash
cargo test --test full_cycle_test
```

## License
MIT License. See [LICENSE](LICENSE) for details.
