use scp_economics::{
    staking::{StakingConfig, StakingPool, StakingMetrics},
    token::CtdlToken,
    fees::{FeeConfig, FeeDistributor},
};
use scp_signer::JsonKeystore;
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
    keystore_path: PathBuf,
    token_path: PathBuf,
    staking_path: PathBuf,
    fees_path: PathBuf,
    keystore: Mutex<Option<Arc<JsonKeystore>>>,
    last_action: Mutex<std::time::SystemTime>,
}

impl AppState {
    fn new(app_data_dir: PathBuf) -> Self {
        // Ensure dir exists
        std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data dir");

        let token_path = app_data_dir.join("token.json");
        let staking_path = app_data_dir.join("staking.json");
        let fees_path = app_data_dir.join("fees.json");
        let keystore_path = app_data_dir.join("keystore.json");

        let token = CtdlToken::load_or_new(token_path.clone(), 1_000_000_000);
        
        let pool = StakingPool::load_or_new(
            staking_path.clone(), 
            StakingConfig::default(), 
            token.clone()
        );

        let fees = FeeDistributor::load_or_new(
            fees_path.clone(), 
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
            keystore_path,
            token_path,
            staking_path,
            fees_path,
            keystore: Mutex::new(None),
            last_action: Mutex::new(std::time::SystemTime::now()),
        }
    }

    fn touch(&self) {
        *self.last_action.lock().unwrap() = std::time::SystemTime::now();
    }

    fn check_timeout(&self, timeout_secs: u64) {
        let last = *self.last_action.lock().unwrap();
        if let Ok(elapsed) = last.elapsed() {
            if elapsed.as_secs() > timeout_secs {
                let mut ks = self.keystore.lock().unwrap();
                if ks.is_some() {
                    *ks = None;
                    tracing::info!("Session timeout: wallet locked.");
                }
            }
        }
    }

    fn save_all(&self) -> tauri::Result<()> {
        self.token.save(&self.token_path).ok();
        self.staking_pool.save(&self.staking_path).ok();
        self.fee_distributor.save(&self.fees_path).ok();
        Ok(())
    }
}

// --- Commands ---

#[tauri::command]
fn is_wallet_initialized(state: State<'_, Arc<AppState>>) -> bool {
    state.touch();
    state.keystore_path.exists()
}

#[tauri::command]
fn unlock_wallet(password: String, state: State<'_, Arc<AppState>>) -> std::result::Result<bool, String> {
    state.touch();
    let keystore = JsonKeystore::new(state.keystore_path.clone(), password)
        .map_err(|e| e.to_string())?;
    
    *state.keystore.lock().unwrap() = Some(Arc::new(keystore));
    Ok(true)
}

#[tauri::command]
fn lock_wallet(state: State<'_, Arc<AppState>>) {
    *state.keystore.lock().unwrap() = None;
}

#[tauri::command]
fn is_wallet_locked(state: State<'_, Arc<AppState>>) -> bool {
    state.keystore.lock().unwrap().is_none()
}

#[tauri::command]
fn get_sentiment_history(state: State<'_, Arc<AppState>>) -> Vec<SentimentPoint> {
    let mut history = state.history.lock().unwrap();
    
    // Simulate live update
    if history.len() < 50 { 
        let next_len = history.len() + 10;
        let new_score = 0.5;
        history.push(SentimentPoint {
            timestamp: format!("{}:{}", 10, next_len),
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
fn get_governance_state(state: State<'_, Arc<AppState>>) -> GovernanceState {
    state.touch();
    let metrics = state.staking_pool.metrics();
    
    let demo_pubkey = demo_user_key();
    
    let (my_staked, my_pending) = if let Some(pos) = state.staking_pool.position(&demo_pubkey) {
        (pos.amount, pos.pending_rewards.0)
    } else {
        (0, 0)
    };

    GovernanceState {
        total_staked: metrics.total_staked,
        stakers_count: metrics.total_stakers,
        total_rewards: metrics.total_rewards_distributed.0,
        my_staked,
        my_pending_rewards: my_pending,
    }
}

#[tauri::command]
fn stake(amount: u64, state: State<'_, Arc<AppState>>) -> std::result::Result<String, String> {
    let user = demo_user_key();
    let _ = state.token.mint(&user, amount + 100); 
    
    state.staking_pool.stake(&user, amount)
        .map(|_| {
            let _ = state.save_all(); // Auto-save
            "Staked successfully".to_string()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn claim(state: State<'_, Arc<AppState>>) -> std::result::Result<u64, String> {
    let user = demo_user_key();
    state.staking_pool.claim(&user)
        .map(|s| {
            let _ = state.save_all(); // Auto-save
            s.0
        })
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
struct VaultMetrics {
    tvl_btc: f64,
    active_dlcs: usize,
    circuit_breaker: bool,
    spread: f64,
}

#[tauri::command]
fn get_vault_metrics(_state: State<'_, Arc<AppState>>) -> VaultMetrics {
    // In a real app, query LiquidityPool and DlcManager here
    VaultMetrics {
        tvl_btc: 1.25, // Mocked 1.25 BTC
        active_dlcs: 3,
        circuit_breaker: false,
        spread: 0.02, // 2%
    }
}

fn demo_user_key() -> scp_core::PublicKey {
    use std::str::FromStr;
    scp_core::PublicKey::from_str("0250863ad64a87ae8a2fe83c1af1a8403cb53f53e486d8511dad8a04887e5b2352").unwrap()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            println!("Tauri setup starting...");
            let app_dir = app.path().app_data_dir().map_err(|e| {
                println!("Error getting app_data_dir: {:?}", e);
                e
            }).unwrap();
            println!("App data dir: {:?}", app_dir);
            let state = AppState::new(app_dir);
            println!("AppState initialized.");
            
            // Auto-lock checker (every 30 seconds)
            let state_handle = Arc::new(state);
            let state_checker = state_handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    state_checker.check_timeout(300); // 5 minute timeout
                }
            });

            app.manage(state_handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_sentiment_history,
            get_governance_state,
            get_vault_metrics,
            stake,
            claim,
            is_wallet_initialized,
            unlock_wallet,
            lock_wallet,
            is_wallet_locked
        ]);

    builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state: State<AppState> = app_handle.state();
                let _ = state.save_all();
            }
        });
}
