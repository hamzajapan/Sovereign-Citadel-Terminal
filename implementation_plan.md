# Phase 6 Implementation Plan: Economics & CLI

## Goal
Implement $CTDL token economics (staking, fee distribution) and create a polished CLI for demonstrating the full protocol lifecycle.

## User Review Required
None.

## Proposed Changes

### [Component] `scp-cli`
- [NEW] Implement `token` subcommands (`mint`, `transfer`, `balance`) using `scp_economics`.
- [NEW] Implement `staking` subcommands (`stake`, `unstake`, `claim`) connecting to `StakingPool`.
- [NEW] Implement `governance` subcommands (`distribute-fees`) connecting to `FeeDistributor`.
- [MODIFY] `main.rs`: Initialize `CtdlToken`, `StakingPool`, and `FeeDistributor` in `run` mode.

### [Component] `scp-economics`
- [MODIFY] `staking.rs` & `fees.rs`: Ensure compatibility with CLI.

## Verification Plan
- **Manual Verification (CLI Demo)**:
    1.  Start node: `scp run`
    2.  Mint tokens: `scp token mint --to alice --amount 1000`
    3.  Stake tokens: `scp staking stake --amount 500`
    4.  Simulate fee: `scp governance distribute-fees --amount 100`
    5.  Check rewards: `scp staking status` -> expect rewards
    6.  Claim rewards: `scp staking claim`

