use crate::sentiment::{SentimentProvider, SentimentScore};
use async_trait::async_trait;
use scp_core::{Error, Result};
use std::collections::HashMap;

pub struct RssSentimentProvider {
    rss_url: String,
    keywords: HashMap<String, f64>,
}

impl RssSentimentProvider {
    pub fn new(rss_url: &str) -> Self {
        let mut keywords = HashMap::new();
        // Fear words
        keywords.insert("crash".to_string(), -0.8);
        keywords.insert("ban".to_string(), -0.9);
        keywords.insert("hack".to_string(), -0.95);
        keywords.insert("regulatory".to_string(), -0.3);
        keywords.insert("sell".to_string(), -0.4);
        keywords.insert("bear".to_string(), -0.5);
        keywords.insert("drop".to_string(), -0.5);
        keywords.insert("plummet".to_string(), -0.7);

        // Greed words
        keywords.insert("bull".to_string(), 0.6);
        keywords.insert("adoption".to_string(), 0.7);
        keywords.insert("record".to_string(), 0.5);
        keywords.insert("high".to_string(), 0.4);
        keywords.insert("buy".to_string(), 0.4);
        keywords.insert("surge".to_string(), 0.7);
        keywords.insert("etf".to_string(), 0.5);
        keywords.insert("approve".to_string(), 0.6);

        Self {
            rss_url: rss_url.to_string(),
            keywords,
        }
    }
}

#[async_trait]
impl SentimentProvider for RssSentimentProvider {
    async fn fetch_sentiment(&self) -> Result<SentimentScore> {
        let content = reqwest::get(&self.rss_url)
            .await
            .map_err(|e| Error::SentimentProvider(format!("Failed to fetch RSS: {}", e)))?
            .bytes()
            .await
            .map_err(|e| Error::SentimentProvider(format!("Failed to read RSS bytes: {}", e)))?;

        let channel = rss::Channel::read_from(&content[..])
            .map_err(|e| Error::SentimentProvider(format!("Failed to parse RSS: {}", e)))?;

        let mut total_score = 0.0;
        let mut count = 0;

        // Analyze last 20 headlines
        for item in channel.items().iter().take(20) {
            let title = item.title().unwrap_or("").to_lowercase();
            let description = item.description().unwrap_or("").to_lowercase();
            let text = format!("{} {}", title, description);
            
            let mut item_score = 0.0;
            let mut matches = 0;

            for (word, score) in &self.keywords {
                if text.contains(word) {
                    item_score += score;
                    matches += 1;
                }
            }

            if matches > 0 {
                total_score += item_score / matches as f64; // Average score for this item
                count += 1;
            }
        }

        let final_value = if count > 0 {
            (total_score / count as f64).clamp(-1.0, 1.0)
        } else {
            0.0 // Neutral if no keywords found
        };

        // Simple confidence based on volume of data points
        let confidence = (count as f64 / 20.0).clamp(0.1, 1.0); 

        Ok(SentimentScore {
            value: final_value,
            confidence,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }
}
