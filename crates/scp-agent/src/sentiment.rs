//! Sentiment analysis providers.
//!
//! Provides interfaces for analyzing market sentiment.

use async_trait::async_trait;
use scp_core::Result;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Sentiment score normalized between -1.0 (Extreme Fear) and 1.0 (Extreme Greed).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SentimentScore {
    pub value: f64,
    pub confidence: f64,
    pub timestamp: u64,
}

impl SentimentScore {
    pub fn is_fear(&self) -> bool {
        self.value < -0.5
    }

    pub fn is_greed(&self) -> bool {
        self.value > 0.5
    }

    pub fn neutral() -> Self {
        Self {
            value: 0.0,
            confidence: 0.5,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    pub fn is_high_risk(&self) -> bool {
        self.value < -0.8 && self.confidence > 0.7
    }
}

#[async_trait]
pub trait SentimentProvider: Send + Sync {
    /// Fetch the current market sentiment.
    async fn fetch_sentiment(&self) -> Result<SentimentScore>;
}

/// A mock provider for testing and development.
pub struct MockSentimentProvider {
    current_value: Arc<Mutex<f64>>,
}

impl MockSentimentProvider {
    pub fn new(initial_value: f64) -> Self {
        Self {
            current_value: Arc::new(Mutex::new(initial_value)),
        }
    }

    pub fn set_sentiment(&self, value: f64) {
        *self.current_value.lock().unwrap() = value;
    }
}

impl Default for MockSentimentProvider {
    fn default() -> Self {
        Self::new(0.0)
    }
}

#[async_trait]
impl SentimentProvider for MockSentimentProvider {
    async fn fetch_sentiment(&self) -> Result<SentimentScore> {
        let val = *self.current_value.lock().unwrap();
        Ok(SentimentScore {
            value: val,
            confidence: 1.0,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }
}

/// A web-based sentiment provider (placeholder for real API).
pub struct WebSentimentProvider {
    _client: reqwest::Client,
    _api_url: String,
}

impl WebSentimentProvider {
    pub fn new(api_url: &str) -> Self {
        Self {
            _client: reqwest::Client::new(),
            _api_url: api_url.to_string(),
        }
    }
}

#[async_trait]
impl SentimentProvider for WebSentimentProvider {
    async fn fetch_sentiment(&self) -> Result<SentimentScore> {
        // In a real implementation, we would call the API.
        // For now, return a neutral score.
        // let _resp = self.client.get(&self.api_url).send().await;

        Ok(SentimentScore {
            value: 0.0,
            confidence: 0.5,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }
}
