use scp_core::{Satoshi, Result};
use scp_economics::{
    staking::{StakingConfig, StakingPool, StakingMetrics},
    token::CtdlToken,
    fees::{FeeConfig, FeeDistributor},
};
use std::sync::{Arc, Mutex};
use tauri::{State, Manager};
use std::path::PathBuf;

#[derive(serde::Serialize, Clone)]
struct SentimentPoint {
    timestamp: String,
    score: f64,
}

// --- App State ---
struct AppState {
    token: Arc<CtdlToken>,
    staking_pool: Arc<StakingPool>,
    fee_distributor: Arc<FeeDistributor>,
    history: Mutex<Vec<SentimentPoint>>,
}

impl AppState {
    fn new(app_data_dir: PathBuf) -> Self {
        // Ensure dir exists
        std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data dir");

        let token_path = app_data_dir.join("token.json");
        let staking_path = app_data_dir.join("staking.json");
        let fees_path = app_data_dir.join("fees.json");

        let token = CtdlToken::load_or_new(token_path, 1_000_000_000).expect("Failed to load token");
        
        let pool = StakingPool::load_or_new(
            staking_path, 
            StakingConfig::default(), 
            token.clone()
        );

        let fees = FeeDistributor::load_or_new(
            fees_path, 
            FeeConfig::default(), 
            token.clone()
        );

        // Initial mock history
        let history = vec![
            SentimentPoint { timestamp: "10:00".into(), score: 0.5 },
            SentimentPoint { timestamp: "10:05".into(), score: 0.2 },
            SentimentPoint { timestamp: "10:10".into(), score: -0.4 },
        ];

        Self {
            token,
            staking_pool: Arc::new(pool),
            fee_distributor: Arc::new(fees),
            history: Mutex::new(history),
        }
    }
}

// --- Commands ---

#[tauri::command]
fn get_sentiment_history(state: State<AppState>) -> Vec<SentimentPoint> {
    let mut history = state.history.lock().unwrap();
    
    // Simulate live update: append a new point occasionally
    // In a real app, the background agent loop would push to this
    let last_time = history.last().map(|p| p.timestamp.clone()).unwrap_or("10:00".to_string());
    // Very simple mock logic to advance time/score
    if history.len() < 50 { 
        let new_score = (rand::random::<f64>() * 2.0) - 1.0; // -1 to 1
        history.push(SentimentPoint {
            timestamp: format!("{}:{}", 10, history.len() + 10),
            score: new_score,
        });
    }

    history.clone()
}

#[derive(serde::Serialize)]
struct GovernanceState {
    total_staked: u64,
    stakers_count: usize,
    total_rewards: u64, // sats
    my_staked: u64,
    my_pending_rewards: u64, // sats
}

#[tauri::command]
fn get_governance_state(state: State<AppState>) -> GovernanceState {
    let metrics = state.staking_pool.metrics();
    
    let demo_pubkey = demo_user_key();
    
    let (my_staked, my_pending) = if let Some(pos) = state.staking_pool.position(&demo_pubkey) {
        (pos.amount, pos.pending_rewards.as_sat())
    } else {
        (0, 0)
    };

    GovernanceState {
        total_staked: metrics.total_staked,
        stakers_count: metrics.total_stakers,
        total_rewards: metrics.total_rewards_distributed.as_sat(),
        my_staked,
        my_pending_rewards: my_pending,
    }
}

#[tauri::command]
fn stake(amount: u64, state: State<AppState>) -> std::result::Result<String, String> {
    let user = demo_user_key();
    let _ = state.token.mint(&user, amount + 100); 

    state.staking_pool.stake(&user, amount)
        .map(|_| "Staked successfully".to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn claim(state: State<AppState>) -> std::result::Result<u64, String> {
    let user = demo_user_key();
    state.staking_pool.claim(&user)
        .map(|s| s.as_sat())
        .map_err(|e| e.to_string())
}

fn demo_user_key() -> scp_core::PublicKey {
    use std::str::FromStr;
    scp_core::PublicKey::from_str("0250863ad64a87ae8a2fe83c1af1a8403cb53f53e486d8511dad8a04887e5b2352").unwrap()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_dir = app.path().app_data_dir().unwrap();
            let state = AppState::new(app_dir);
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_sentiment_history,
            get_governance_state,
            stake,
            claim
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
