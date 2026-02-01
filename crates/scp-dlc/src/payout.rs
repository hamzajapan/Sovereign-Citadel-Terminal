//! Payout curve definitions.
//!
//! Defines how funds are distributed based on oracle outcomes.

use scp_core::Satoshi;
use serde::{Deserialize, Serialize};

/// A payout curve defining how funds are distributed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PayoutCurve {
    /// Binary outcome (win/lose).
    Binary {
        /// Amount paid to winner.
        win_amount: Satoshi,
        /// Amount paid to loser (usually 0).
        lose_amount: Satoshi,
    },

    /// Linear payout based on numeric outcome.
    Linear {
        /// The outcome value where offerer gets everything.
        offerer_wins_at: i64,
        /// The outcome value where accepter gets everything.
        accepter_wins_at: i64,
        /// Total collateral.
        total_collateral: Satoshi,
    },

    /// Discrete outcomes with explicit payouts.
    Discrete {
        /// List of (outcome, offerer_payout) pairs.
        points: Vec<PayoutPoint>,
    },

    /// Polynomial curve for more complex payouts.
    Polynomial {
        /// Polynomial coefficients [a0, a1, a2, ...] for a0 + a1*x + a2*x^2 + ...
        coefficients: Vec<f64>,
        /// Minimum payout.
        min_payout: Satoshi,
        /// Maximum payout.
        max_payout: Satoshi,
    },
}

/// A discrete payout point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayoutPoint {
    /// The outcome value.
    pub outcome: i64,
    /// Payout to the offerer at this outcome.
    pub offerer_payout: Satoshi,
}

impl PayoutCurve {
    /// Calculate the offerer's payout for a given outcome.
    pub fn calculate_offerer_payout(&self, outcome: i64, _total_collateral: Satoshi) -> Satoshi {
        match self {
            PayoutCurve::Binary {
                win_amount,
                lose_amount,
            } => {
                if outcome > 0 {
                    *win_amount
                } else {
                    *lose_amount
                }
            }

            PayoutCurve::Linear {
                offerer_wins_at,
                accepter_wins_at,
                total_collateral,
            } => {
                if outcome >= *offerer_wins_at {
                    *total_collateral
                } else if outcome <= *accepter_wins_at {
                    Satoshi::ZERO
                } else {
                    // Linear interpolation
                    let range = (offerer_wins_at - accepter_wins_at) as f64;
                    let position = (outcome - accepter_wins_at) as f64;
                    let ratio = position / range;
                    Satoshi::from_sat((total_collateral.as_sat() as f64 * ratio) as u64)
                }
            }

            PayoutCurve::Discrete { points } => {
                // Find the closest point
                points
                    .iter()
                    .min_by_key(|p| (p.outcome - outcome).abs())
                    .map(|p| p.offerer_payout)
                    .unwrap_or(Satoshi::ZERO)
            }

            PayoutCurve::Polynomial {
                coefficients,
                min_payout,
                max_payout,
            } => {
                let x = outcome as f64;
                let mut result = 0.0;
                for (i, coeff) in coefficients.iter().enumerate() {
                    result += coeff * x.powi(i as i32);
                }
                let sats = result as u64;
                Satoshi::from_sat(sats.clamp(min_payout.as_sat(), max_payout.as_sat()))
            }
        }
    }

    /// Calculate the accepter's payout (complement of offerer).
    pub fn calculate_accepter_payout(&self, outcome: i64, total_collateral: Satoshi) -> Satoshi {
        let offerer = self.calculate_offerer_payout(outcome, total_collateral);
        total_collateral
            .checked_sub(offerer)
            .unwrap_or(Satoshi::ZERO)
    }

    /// Validate the payout curve.
    pub fn validate(&self, total_collateral: Satoshi) -> Result<(), String> {
        match self {
            PayoutCurve::Binary {
                win_amount,
                lose_amount,
            } => {
                if *win_amount + *lose_amount != total_collateral {
                    return Err("Win + Lose must equal total collateral".to_string());
                }
            }
            PayoutCurve::Linear {
                offerer_wins_at,
                accepter_wins_at,
                ..
            } => {
                if offerer_wins_at <= accepter_wins_at {
                    return Err("offerer_wins_at must be > accepter_wins_at".to_string());
                }
            }
            PayoutCurve::Discrete { points } => {
                if points.is_empty() {
                    return Err("Discrete curve must have at least one point".to_string());
                }
            }
            PayoutCurve::Polynomial { coefficients, .. } => {
                if coefficients.is_empty() {
                    return Err("Polynomial must have at least one coefficient".to_string());
                }
            }
        }
        Ok(())
    }
}

/// Builder for creating payout curves.
pub struct PayoutCurveBuilder {
    total_collateral: Satoshi,
}

impl PayoutCurveBuilder {
    /// Create a new builder.
    pub fn new(total_collateral: Satoshi) -> Self {
        Self { total_collateral }
    }

    /// Create a simple binary (win/lose) curve.
    pub fn binary_winner_takes_all(self) -> PayoutCurve {
        PayoutCurve::Binary {
            win_amount: self.total_collateral,
            lose_amount: Satoshi::ZERO,
        }
    }

    /// Create a binary curve with specified split.
    pub fn binary(self, win_ratio: f64) -> PayoutCurve {
        let win = Satoshi::from_sat((self.total_collateral.as_sat() as f64 * win_ratio) as u64);
        let lose = self.total_collateral - win;
        PayoutCurve::Binary {
            win_amount: win,
            lose_amount: lose,
        }
    }

    /// Create a linear curve between two price points.
    pub fn linear(self, offerer_wins_at: i64, accepter_wins_at: i64) -> PayoutCurve {
        PayoutCurve::Linear {
            offerer_wins_at,
            accepter_wins_at,
            total_collateral: self.total_collateral,
        }
    }

    /// Create a discrete curve from outcome-payout pairs.
    pub fn discrete(self, points: Vec<(i64, u64)>) -> PayoutCurve {
        PayoutCurve::Discrete {
            points: points
                .into_iter()
                .map(|(outcome, sats)| PayoutPoint {
                    outcome,
                    offerer_payout: Satoshi::from_sat(sats),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_payout() {
        let total = Satoshi::from_sat(2_000_000);
        let curve = PayoutCurveBuilder::new(total).binary_winner_takes_all();

        // Positive outcome = offerer wins
        assert_eq!(
            curve.calculate_offerer_payout(1, total),
            Satoshi::from_sat(2_000_000)
        );
        assert_eq!(curve.calculate_accepter_payout(1, total), Satoshi::ZERO);

        // Zero/negative outcome = accepter wins
        assert_eq!(curve.calculate_offerer_payout(0, total), Satoshi::ZERO);
        assert_eq!(
            curve.calculate_accepter_payout(0, total),
            Satoshi::from_sat(2_000_000)
        );
    }

    #[test]
    fn test_linear_payout() {
        let total = Satoshi::from_sat(1_000_000);
        let curve = PayoutCurveBuilder::new(total).linear(60000, 40000);

        // At offerer_wins_at
        assert_eq!(
            curve.calculate_offerer_payout(60000, total),
            Satoshi::from_sat(1_000_000)
        );

        // At accepter_wins_at
        assert_eq!(curve.calculate_offerer_payout(40000, total), Satoshi::ZERO);

        // Midpoint
        let mid_payout = curve.calculate_offerer_payout(50000, total);
        assert_eq!(mid_payout, Satoshi::from_sat(500_000));
    }

    #[test]
    fn test_discrete_payout() {
        let total = Satoshi::from_sat(1_000_000);
        let curve = PayoutCurveBuilder::new(total).discrete(vec![
            (100, 0),
            (200, 500_000),
            (300, 1_000_000),
        ]);

        assert_eq!(curve.calculate_offerer_payout(100, total), Satoshi::ZERO);
        assert_eq!(
            curve.calculate_offerer_payout(300, total),
            Satoshi::from_sat(1_000_000)
        );
    }
}
