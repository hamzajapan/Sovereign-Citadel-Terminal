use crate::sentiment::{
    MockSentimentProvider, SentimentProvider, SentimentScore, WebSentimentProvider,
};
use crate::rss_provider::RssSentimentProvider;
use crate::spread::SpreadCalculator;
use scp_core::channels::{AgentSignal, VaultEvent};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

pub struct AgentConfig {
    pub use_web_sentiment: bool,
    pub use_rss_sentiment: bool,
    pub api_url: String,
    pub rss_url: String,
    pub poll_interval_secs: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            use_web_sentiment: false,
            use_rss_sentiment: false,
            api_url: "https://api.coingecko.com/api/v3/ping".to_string(),
            rss_url: "https://www.coindesk.com/arc/outboundfeeds/rss/".to_string(),
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
        let sentiment_provider: Arc<dyn SentimentProvider> = if config.use_rss_sentiment {
            Arc::new(RssSentimentProvider::new(&config.rss_url))
        } else if config.use_web_sentiment {
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
        
        let (sentiment_tx, mut sentiment_rx) = mpsc::channel(32);
        let provider = self.sentiment_provider.clone();
        let interval = Duration::from_secs(self.config.poll_interval_secs);

        // 1. Spawn Background Worker for Sentiment (RSS/Web)
        tokio::spawn(async move {
            loop {
                match provider.fetch_sentiment().await {
                    Ok(score) => {
                        if let Err(e) = sentiment_tx.send(score).await {
                            error!("Sentiment worker failed to send: {}", e);
                            break;
                        }
                    }
                    Err(e) => error!("Sentiment worker fetch failed: {}", e),
                }
                sleep(interval).await;
            }
        });

        // 2. Reactive Main Loop
        loop {
            tokio::select! {
                // Handle new sentiment scores
                Some(score) = sentiment_rx.recv() => {
                    self.current_sentiment = score;
                    info!(
                        "Sentiment: {:.2} (Conf: {:.2})",
                        score.value, score.confidence
                    );

                    if score.is_high_risk() {
                        warn!("High risk detected! Triggering circuit breaker.");
                        self.emit_signal(AgentSignal::CircuitBreaker {
                            reason: "Extreme Fear Sentiment".into(),
                            duration_secs: Some(3600),
                        })
                        .await;
                    } else {
                        let spread_factor = self
                            .spread_calculator
                            .calculate_multiplier(score.value, 0.5);
                        if (spread_factor - 1.0).abs() > 0.01 {
                            self.emit_signal(AgentSignal::WidenSpread {
                                factor: spread_factor,
                            })
                            .await;
                        }
                    }
                }

                // Handle protocol events (e.g. from Vault)
                Some(event) = async {
                    if let Some(rx) = &mut self.event_rx {
                        rx.recv().await
                    } else {
                        None
                    }
                } => {
                    info!("Agent received vault event: {:?}", event);
                    // Process events (e.g. re-calculate spreads if liquidity changes significantly)
                }
            }
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
