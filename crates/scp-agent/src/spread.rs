#[derive(Debug, Clone)]
pub struct SpreadCalculator {
    base_spread: f64,
    min_spread: f64,
    max_spread: f64,
}

impl SpreadCalculator {
    pub fn new(base_spread: f64) -> Self {
        Self {
            base_spread,
            min_spread: 0.001, // 0.1%
            max_spread: 0.05,  // 5%
        }
    }

    /// Calculate the target spread based on sentiment (-1.0 to 1.0) and volatility.
    pub fn calculate(&self, sentiment: f64, volatility: f64) -> f64 {
        let mut spread = self.base_spread;

        // 1. Adjust for Sentiment
        // Fear (-1.0) -> Widen spread significantly
        // Greed (1.0) -> Widen slightly (profit taking) or keep tight
        if sentiment < -0.5 {
            spread *= 2.0; // Panic mode
        } else if sentiment < 0.0 {
            spread *= 1.2; // Caution
        }

        // 2. Adjust for Volatility
        if volatility > 0.8 {
            spread *= 1.5;
        }

        // Clamp
        spread.clamp(self.min_spread, self.max_spread)
    }

    pub fn calculate_multiplier(&self, sentiment: f64, volatility: f64) -> f64 {
        let spread = self.calculate(sentiment, volatility);
        spread / self.base_spread
    }
}

impl Default for SpreadCalculator {
    fn default() -> Self {
        Self::new(0.005) // 0.5% base
    }
}
