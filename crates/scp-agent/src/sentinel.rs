use crate::sentiment::{
    MockSentimentProvider, SentimentProvider, SentimentScore, WebSentimentProvider,
};
use crate::spread::SpreadCalculator;
use scp_core::channels::{AgentSignal, VaultEvent};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

pub struct AgentConfig {
    pub use_web_sentiment: bool,
    pub api_url: String,
    pub poll_interval_secs: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            use_web_sentiment: false,
            api_url: "https://api.coingecko.com/api/v3/ping".to_string(),
            poll_interval_secs: 60,
        }
    }
}

pub struct CitadelAgent {
    config: AgentConfig,
    sentiment_provider: Arc<dyn SentimentProvider>,
    spread_calculator: SpreadCalculator,
    signal_tx: Option<mpsc::Sender<AgentSignal>>,
    event_rx: Option<mpsc::Receiver<VaultEvent>>,
    // Internal state
    current_sentiment: SentimentScore,
}

impl CitadelAgent {
    pub fn new_with_provider(config: AgentConfig, provider: Arc<dyn SentimentProvider>) -> Self {
        Self {
            config,
            sentiment_provider: provider,
            spread_calculator: SpreadCalculator::default(),
            signal_tx: None,
            event_rx: None,
            current_sentiment: SentimentScore::neutral(),
        }
    }

    pub fn new(config: AgentConfig) -> Self {
        let sentiment_provider: Arc<dyn SentimentProvider> = if config.use_web_sentiment {
            Arc::new(WebSentimentProvider::new(&config.api_url))
        } else {
            Arc::new(MockSentimentProvider::new(0.0))
        };
        Self::new_with_provider(config, sentiment_provider)
    }

    pub fn attach_channels(
        &mut self,
        signal_tx: mpsc::Sender<AgentSignal>,
        event_rx: mpsc::Receiver<VaultEvent>,
    ) {
        self.signal_tx = Some(signal_tx);
        self.event_rx = Some(event_rx);
    }

    pub async fn run(&mut self) {
        info!("Citadel Agent starting...");
        let interval = Duration::from_secs(self.config.poll_interval_secs);

        loop {
            // 1. Fetch Sentiment
            match self.sentiment_provider.fetch_sentiment().await {
                Ok(score) => {
                    self.current_sentiment = score;
                    info!(
                        "Sentiment: {:.2} (Conf: {:.2})",
                        score.value, score.confidence
                    );

                    // 2. Risk Logic
                    if score.is_high_risk() {
                        warn!("High risk detected! Triggering circuit breaker.");
                        self.emit_signal(AgentSignal::CircuitBreaker {
                            reason: "Extreme Fear Sentiment".into(),
                            duration_secs: Some(3600), // Pause for 1 hour
                        })
                        .await;
                    } else {
                        // Adjust spread
                        let spread_factor = self
                            .spread_calculator
                            .calculate_multiplier(score.value, 0.5); // Fixed vol for now
                                                                     // Only emit if factor implies a change (e.g. > 1.05 or < 0.95)
                        if (spread_factor - 1.0).abs() > 0.01 {
                            self.emit_signal(AgentSignal::WidenSpread {
                                factor: spread_factor,
                            })
                            .await;
                        }
                    }
                }
                Err(e) => error!("Failed to fetch sentiment: {}", e),
            }

            sleep(interval).await;
        }
    }

    async fn emit_signal(&self, signal: AgentSignal) {
        if let Some(tx) = &self.signal_tx {
            if let Err(e) = tx.send(signal).await {
                error!("Failed to send signal: {}", e);
            }
        }
    }

    // Proxy for internal access
    pub async fn current_sentiment(&self) -> SentimentScore {
        self.current_sentiment
    }

    pub fn spread_calculator(&self) -> &SpreadCalculator {
        &self.spread_calculator
    }

    pub fn set_test_sentiment(&self, _value: f64) {
        // Only works if provider is mock, but we can't easily downcast Arc<dyn>.
        // Ideally we would expose this via the trait or handle it differently.
        // For MVP, simplistic approach.
        // Actually, we can update the internal score immediately
        // self.current_sentiment = SentimentScore { value, confidence: 1.0, timestamp: 0 };
        // But the provider is what 'run' uses.
        // Ignored for now.
    }
}
