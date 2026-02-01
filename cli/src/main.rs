//! # Sovereign Citadel Protocol CLI
//!
//! Command-line interface for interacting with the SCP protocol.

use clap::{Parser, Subcommand};
use scp_agent::{AgentConfig, CitadelAgent};
use scp_chain::mock::MockBlockchain;
use scp_core::{PublicKey, Satoshi};
use scp_dlc::oracle::MockOracle;
use scp_dlc::state_machine::DlcStateMachine;
use scp_dlc::storage::DlcStorage;
use scp_dlc::DlcManager;
use scp_economics::{
    fees::FeeConfig, staking::StakingConfig, CtdlToken, FeeDistributor, StakingPool,
};
use scp_signer::{keystore::MemoryKeystore, Signer};
use scp_vault::pool::{LiquidityPool, PoolConfig};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "scp")]
#[command(author = "Sovereign Citadel")]
#[command(version = "0.1.0")]
#[command(about = "Sovereign Citadel Protocol CLI", long_about = None)]
struct Cli {
    /// Config file path
    #[arg(short, long, default_value = "config/default.toml")]
    config: PathBuf,

    /// Data directory
    #[arg(short, long)]
    data_dir: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new SCP node
    Init {
        /// Network (mainnet, testnet, signet, regtest)
        #[arg(short, long, default_value = "signet")]
        network: String,
    },

    /// Vault operations
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },

    /// DLC operations
    Dlc {
        #[command(subcommand)]
        action: DlcAction,
    },

    /// Agent operations
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },

    /// Token operations
    Token {
        #[command(subcommand)]
        action: TokenAction,
    },

    /// Governance operations
    Governance {
        #[command(subcommand)]
        action: GovernanceAction,
    },

    /// Run the full node (vault + agent)
    Run,
}

#[derive(Subcommand)]
enum VaultAction {
    /// Show vault status
    Status,
    /// Deposit to vault
    Deposit {
        #[arg(short, long)]
        amount: u64,
        /// Enable delta-neutral hedging
        #[arg(long)]
        auto_hedge: bool,
    },
    /// Withdraw from vault
    Withdraw {
        #[arg(short, long)]
        shares: u64,
    },
    /// Show pool metrics
    Metrics,
}

#[derive(Subcommand)]
enum DlcAction {
    /// List all contracts
    List {
        /// Filter by state
        #[arg(short, long)]
        state: Option<String>,
    },
    /// Show contract details
    Show {
        /// Contract ID (hex)
        id: String,
    },
    /// Create a new contract offer
    Offer {
        /// Collateral in satoshis
        #[arg(short, long)]
        collateral: u64,
        /// Event descriptor
        #[arg(short, long)]
        event: String,
    },
}

#[derive(Subcommand)]
enum AgentAction {
    /// Show agent status
    Status,
    /// Set sentiment for testing
    SetSentiment {
        #[arg(short, long)]
        value: f64,
    },
}

#[derive(Subcommand)]
enum TokenAction {
    /// Show token balance
    Balance {
        #[arg(short, long)]
        holder: String,
    },
    /// Mint tokens (Testnet only)
    Mint {
        #[arg(short, long)]
        to: String,
        #[arg(short, long)]
        amount: u64,
    },
    /// Transfer tokens
    Transfer {
        #[arg(short, long)]
        to: String,
        #[arg(short, long)]
        amount: u64,
    },
    /// Stake tokens
    Stake {
        #[arg(short, long)]
        amount: u64,
    },
    /// Unstake tokens
    Unstake {
        #[arg(short, long)]
        amount: u64,
    },
    /// Claim rewards
    ClaimRewards,
    /// Show Staking Status
    Status,
}

#[derive(Subcommand)]
enum GovernanceAction {
    /// Distribute collected fees
    DistributeFees,
    /// Collect fee (Simulated)
    CollectFee {
        #[arg(short, long)]
        amount: u64,
    },
    /// Show fee status
    Status,
}

fn main() {
    let cli = Cli::parse();

    // Setup logging
    let level = if cli.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");

    // Get data directory
    let data_dir = cli.data_dir.unwrap_or_else(|| {
        let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("sovereign-citadel");
        path
    });

    info!(data_dir = ?data_dir, "Starting SCP CLI");

    // 1. Initialize Economics State
    std::fs::create_dir_all(&data_dir).expect("Failed to create data dir");

    let token_path = data_dir.join("token.json");
    let token = CtdlToken::load_or_new(token_path.clone(), 1_000_000_000); // 1B Supply

    let staking_path = data_dir.join("staking.json");
    let staking_pool = Arc::new(StakingPool::load_or_new(
        staking_path.clone(),
        StakingConfig::default(),
        token.clone(),
    ));

    let fees_path = data_dir.join("fees.json");
    let fee_distributor = Arc::new(FeeDistributor::load_or_new(
        fees_path.clone(),
        FeeConfig::default(),
        token.clone(),
    ));

    // Run the command
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create runtime");

    // Create a default signer for CLI user interactions (in a real app, this would be a wallet)
    // For simplicity, we generate a random key for the "current user" or load from a file.
    // Here we'll just mock a user key for simplicity or require it as arg.
    // Let's assume a default "cli_user" keypair is stored or generated.
    let cli_user = get_cli_user_key(&data_dir);

    runtime.block_on(async {
        match cli.command {
            Commands::Init { network } => {
                info!(network = %network, "Initializing SCP node");
                std::fs::create_dir_all(&data_dir).expect("Failed to create data dir");
                std::fs::create_dir_all(data_dir.join("dlc")).expect("Failed to create dlc dir");
                std::fs::create_dir_all(data_dir.join("wallet"))
                    .expect("Failed to create wallet dir");
                println!("✓ SCP node initialized for {} network", network);
            }

            Commands::Token { action } => match action {
                TokenAction::Balance { holder } => {
                    let pk = PublicKey::from_str(&holder).expect("Invalid public key");
                    if let Some(balance) = token.balance_of(&pk) {
                        println!("{}", serde_json::to_string_pretty(&balance).unwrap());
                    } else {
                        println!("No balance found for {}", holder);
                    }
                }
                TokenAction::Mint { to, amount } => {
                    let pk = PublicKey::from_str(&to).expect("Invalid public key");
                    token.mint(&pk, amount).expect("Mint failed");
                    println!("Minted {} CTDL to {}", amount, to);
                    token.save(&token_path).expect("Failed to save token state");
                }
                TokenAction::Transfer { to, amount } => {
                    let pk_to = PublicKey::from_str(&to).expect("Invalid public key");
                    token
                        .transfer(&cli_user, &pk_to, amount)
                        .expect("Transfer failed");
                    println!("Transferred {} CTDL to {}", amount, to);
                    token.save(&token_path).expect("Failed to save token state");
                }
                TokenAction::Stake { amount } => {
                    staking_pool.stake(&cli_user, amount).expect("Stake failed");
                    println!("Staked {} CTDL", amount);
                    token.save(&token_path).expect("Failed to save token state");
                    staking_pool
                        .save(&staking_path)
                        .expect("Failed to save staking state");
                }
                TokenAction::Unstake { amount } => {
                    let returned = staking_pool
                        .unstake(&cli_user, amount)
                        .expect("Unstake failed");
                    println!("Unstaked {} CTDL (net)", returned);
                    token.save(&token_path).expect("Failed to save token state");
                    staking_pool
                        .save(&staking_path)
                        .expect("Failed to save staking state");
                }
                TokenAction::Status => {
                    let metrics = staking_pool.metrics();
                    println!("=== Staking Pool Status ===");
                    println!("Total Staked:  {} CTDL", metrics.total_staked);
                    println!("Stakers:       {}", metrics.total_stakers);
                    println!("Rewards Dist:  {} sats", metrics.total_rewards_distributed);

                    println!("\n=== User Info ===");
                    println!("Public Key:    {}", cli_user);

                    if let Some(pos) = staking_pool.position(&cli_user) {
                        println!("\n--- Your Position ---");
                        println!("Staked:        {} CTDL", pos.amount);
                        println!("Pending Rewards: {} sats", pos.pending_rewards);
                        println!("Locked:        {}", pos.is_locked());
                    }
                }
                TokenAction::ClaimRewards => {
                    let amount = staking_pool.claim(&cli_user).expect("Claim failed");
                    println!("Claimed {} sats", amount);
                    staking_pool
                        .save(&staking_path)
                        .expect("Failed to save staking state");
                }
            },

            Commands::Governance { action } => match action {
                GovernanceAction::DistributeFees => {
                    let amount = fee_distributor
                        .distribute(&staking_pool)
                        .expect("Distribute failed");
                    println!("Distributed {} sats to stakers", amount);
                    fee_distributor
                        .save(&fees_path)
                        .expect("Failed to save fees state");
                    staking_pool
                        .save(&staking_path)
                        .expect("Failed to save staking state");
                }
                GovernanceAction::CollectFee { amount } => {
                    fee_distributor.collect_fee(Satoshi::from_sat(amount));
                    println!("Collected {} sats fee", amount);
                    fee_distributor
                        .save(&fees_path)
                        .expect("Failed to save fees state");
                }
                GovernanceAction::Status => {
                    let metrics = fee_distributor.metrics();
                    println!("=== Fee Distributor Status ===");
                    println!("Pending Fees:    {} sats", metrics.pending_fees);
                    println!("Total Collected: {} sats", metrics.total_collected);
                    println!("Total Dist:      {} sats", metrics.total_distributed);
                }
            },

            Commands::Vault { action } => {
                let pool = Arc::new(LiquidityPool::new(PoolConfig::default()));
                // ... keep existing vault logic ...
                match action {
                    VaultAction::Status => {
                        let metrics = pool.metrics();
                        println!("=== Vault Status ===");
                        println!("Total Liquidity:     {} sats", metrics.total_liquidity);
                        println!("Available:           {} sats", metrics.available_liquidity);
                        println!("Utilization:         {:.1}%", metrics.utilization * 100.0);
                        println!(
                            "Current Spread:      {:.2}%",
                            metrics.current_spread * 100.0
                        );
                    }
                    VaultAction::Deposit { amount, auto_hedge } => {
                        println!("Depositing {} sats (auto-hedge: {})", amount, auto_hedge);
                    }
                    VaultAction::Withdraw { shares } => {
                        println!("Withdrawing {} shares", shares);
                    }
                    VaultAction::Metrics => {
                        let metrics = pool.metrics();
                        println!("{}", serde_json::to_string_pretty(&metrics).unwrap());
                    }
                }
            }

            Commands::Dlc { action } => {
                // ... keep existing DLC logic ...
                let storage = Arc::new(
                    DlcStorage::new(data_dir.join("dlc")).expect("Failed to open DLC storage"),
                );
                let _sm = Arc::new(DlcStateMachine::new(storage.clone()));
                match action {
                    DlcAction::List { state: _ } => {
                        let contracts = storage.list_all().unwrap_or_default();
                        println!("Contracts: {}", contracts.len());
                    }
                    _ => println!("DLC command unimplemented in demo"),
                }
            }

            Commands::Agent { action } => {
                let agent = CitadelAgent::new(AgentConfig::default());
                match action {
                    AgentAction::Status => {
                        let sentiment = agent.current_sentiment().await;
                        println!("Sentiment: {:.2}", sentiment.value);
                    }
                    AgentAction::SetSentiment { value } => {
                        agent.set_test_sentiment(value);
                        println!("Sentiment set to {:.2}", value);
                    }
                }
            }

            Commands::Run => {
                info!("Starting SCP node...");

                let storage = Arc::new(
                    DlcStorage::new(data_dir.join("dlc")).expect("Failed to open DLC storage"),
                );
                let _sm = DlcStateMachine::new(storage.clone());
                let pool = Arc::new(LiquidityPool::new(PoolConfig::default()));

                let (signal_tx, signal_rx) = tokio::sync::mpsc::channel(256);
                let (event_tx, event_rx) = tokio::sync::mpsc::channel(256);

                let mut agent = CitadelAgent::new(AgentConfig::default());
                agent.attach_channels(signal_tx, event_rx);

                let chain = Arc::new(MockBlockchain::new());
                let oracle = Arc::new(MockOracle::new());
                let keystore = MemoryKeystore::new();
                let policy = scp_signer::policy::SigningPolicy::default_policy();
                let signer = Arc::new(Signer::new(Arc::new(keystore), policy));
                let dlc_manager = DlcManager::new(
                    chain,
                    oracle,
                    signer,
                    Arc::new(DlcStateMachine::new(storage)),
                );

                let (vault, handler) =
                    scp_vault::LiquidityVault::new(dlc_manager, signal_rx, event_tx);
                let _vault = vault;

                tokio::spawn(async move {
                    handler.run().await;
                });
                tokio::spawn(async move {
                    agent.run().await;
                });

                println!("=== Sovereign Citadel Protocol ===");
                println!("Node running on signet network");
                println!(
                    "Pool ready:  {} available",
                    pool.metrics().available_liquidity
                );
                println!("Economics:   {} CTDL staked", staking_pool.total_staked());
                println!();
                println!("Press Ctrl+C to stop");

                tokio::signal::ctrl_c()
                    .await
                    .expect("Failed to listen for ctrl+c");
                info!("Shutting down...");
            }
        }
    });
}

// Mock User Key for CLI
fn get_cli_user_key(data_dir: &std::path::Path) -> PublicKey {
    let key_path = data_dir.join("cli_user.key");
    if key_path.exists() {
        let hex_key = std::fs::read_to_string(key_path).expect("Failed to read key");
        PublicKey::from_str(hex_key.trim()).expect("Invalid key file")
    } else {
        let secp = secp256k1::Secp256k1::new();
        let (_, pk) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        let pubkey = PublicKey::new(pk);
        // Ensure parent dir exists
        if let Some(parent) = key_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(key_path, pubkey.to_string()).expect("Failed to write key");
        pubkey
    }
}
