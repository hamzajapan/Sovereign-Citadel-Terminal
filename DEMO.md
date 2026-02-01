# SCP Demo Runbook

This guide references the CLI commands verified during **Phase 6** of development. Use this to demonstrate the end-to-end functionality of the Token Economics and Staking system.

## Prerequisites
- Rust and Cargo installed.
- Terminal with access to the `scp-cli`.

## Steps

### 1. Setup

Clean up any previous test data to start fresh.

```bash
# Windows (PowerShell)
rmdir /s /q test_cli_data
mkdir test_cli_data
```

### 2. Initialize Node

Initialize the SCP node configuration and directory structure.

```bash
cargo run -p scp-cli -- --data-dir test_cli_data init --network signet
```

*Output:* `✓ SCP node initialized for signet network`

### 3. Identify User

Check the initial status to generate and retrieve your unique "CLI User" Public Key.

```bash
cargo run -p scp-cli -- --data-dir test_cli_data token status
```

*Output:* Look for the `Public Key` line, e.g., `02d4f367...`.
**Copy this key.**

### 4. Mint Tokens

Fund your user account with $CTDL tokens (Simulated/Testnet feature). Replace `<KEY>` with the key from Step 3.

```bash
cargo run -p scp-cli -- --data-dir test_cli_data token mint --to <KEY> --amount 1000
```

*Output:* `Minted 1000 CTDL to ...`

### 5. Stake Tokens

Lock tokens in the staking contract to participate in governance and fee distribution.

```bash
cargo run -p scp-cli -- --data-dir test_cli_data token stake --amount 500
```

*Output:* `Staked 500 CTDL`

### 6. Verify Position

Check the status again to confirm your staked position.

```bash
cargo run -p scp-cli -- --data-dir test_cli_data token status
```

*Expected Output:*
```
=== Staking Pool Status ===
Total Staked:  500 CTDL
Stakers:       1
...
=== User Info ===
...
--- Your Position ---
Staked:        500 CTDL
Locked:        true
```

### 7. Distribute Fees (Governance)

Simulate a fee distribution event (normally automated, but triggerable by admin).

```bash
cargo run -p scp-cli -- --data-dir test_cli_data governance distribute-fees
```

*Output:* `Distributed ... sats to stakers`

### 8. Claim Rewards

Claim your accumulated staking rewards.

```bash
cargo run -p scp-cli -- --data-dir test_cli_data token claim
```

*Output:* `Claimed ... sats`
*Verification:* Check `token status` again to see `pending_rewards` reset to 0.

## Running the Protocol Logic

To see the Agent and Vault in action (simulated loop):

```bash
cargo run -p scp-cli -- --data-dir test_cli_data run
```

This will start the async event loop where the Agent monitors sentiment and the Vault manages liquidity.
