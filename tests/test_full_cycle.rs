//! # Integration Tests for SCP Protocol
//!
//! Tests the full DLC lifecycle from offer to settlement.

mod common;

use common::TestHarness;
use scp_core::{ContractId, OracleInfo, Satoshi, Timestamp};
use scp_dlc::contract::{ContractAccept, ContractOffer, ContractSign};
use scp_dlc::oracle::QuorumConfig;
use scp_dlc::payout::PayoutCurveBuilder;
use scp_dlc::state_machine::{DlcState, RefundReason};

#[test]
fn test_full_dlc_lifecycle() {
    let harness = TestHarness::new();

    // === Phase 1: Create Offer ===
    let alice = harness.create_party("Alice");
    let bob = harness.create_party("Bob");
    let oracle = harness.create_oracle("PriceOracle");

    let offer = ContractOffer {
        contract_id: ContractId::from_data(b"btc-usd-jan-2026"),
        offerer: alice.pubkey,
        collateral: Satoshi::from_sat(1_000_000),
        payout_curve: PayoutCurveBuilder::new(Satoshi::from_sat(2_000_000))
            .linear(60000, 40000),
        oracle_info: OracleInfo {
            public_key: oracle.pubkey,
            name: "Price Oracle".to_string(),
            endpoint: Some("https://oracle.example.com".to_string()),
        },
        event_descriptor: "BTC/USD price at 2026-01-31T00:00:00Z".to_string(),
        maturity: Timestamp::from_unix(1738281600), // 2026-01-31
    };

    let state = harness.state_machine.create_offer(offer.clone()).unwrap();
    assert!(matches!(state, DlcState::Offered { .. }));

    // === Phase 2: Accept Offer ===
    let accept = ContractAccept {
        contract_id: offer.contract_id,
        accepter: bob.pubkey,
        collateral: Satoshi::from_sat(1_000_000),
    };

    let state = harness
        .state_machine
        .accept_offer(&offer.contract_id, accept)
        .unwrap();
    assert!(matches!(state, DlcState::Accepted { .. }));

    // === Phase 3: Sign Contract ===
    // === Phase 3: Sign Contract ===
    let sign = ContractSign {
        contract_id: offer.contract_id,
        offerer_signature: harness.sign_message(&alice, offer.contract_id.as_bytes()),
        accepter_signature: harness.sign_message(&bob, offer.contract_id.as_bytes()),
        refund_timeout: Timestamp::from_unix(1738368000), // +24h
    };

    let state = harness
        .state_machine
        .sign_contract(&offer.contract_id, sign)
        .unwrap();
    assert!(matches!(state, DlcState::Signed { .. }));

    // === Phase 4: Confirm Funding ===
    let funding_txid = [1u8; 32];
    let state = harness
        .state_machine
        .confirm_funding(&offer.contract_id, 800000, funding_txid)
        .unwrap();
    assert!(matches!(state, DlcState::Confirmed { .. }));

    // === Phase 5: Oracle Attestation & Settlement ===
    let attestation = harness.create_attestation(&oracle, 55000); // Price = 55000

    let state = harness
        .state_machine
        .settle(&offer.contract_id, attestation, 1_500_000)
        .unwrap();

    match state {
        DlcState::Settled { our_payout, .. } => {
            assert_eq!(our_payout, 1_500_000);
            println!("✓ Contract settled with payout: {} sats", our_payout);
        }
        _ => panic!("Expected Settled state"),
    }
}

#[test]
fn test_contract_refund_on_timeout() {
    let harness = TestHarness::new();

    let alice = harness.create_party("Alice");
    let bob = harness.create_party("Bob");
    let oracle = harness.create_oracle("Oracle");

    // Create and sign contract
    let offer = ContractOffer {
        contract_id: ContractId::from_data(b"refund-test"),
        offerer: alice.pubkey,
        collateral: Satoshi::from_sat(500_000),
        payout_curve: PayoutCurveBuilder::new(Satoshi::from_sat(1_000_000))
            .binary_winner_takes_all(),
        oracle_info: OracleInfo {
            public_key: oracle.pubkey,
            name: "Oracle".to_string(),
            endpoint: None,
        },
        event_descriptor: "Test event".to_string(),
        maturity: Timestamp::from_unix(0), // Already expired
    };

    harness.state_machine.create_offer(offer.clone()).unwrap();
    harness
        .state_machine
        .accept_offer(
            &offer.contract_id,
            ContractAccept {
                contract_id: offer.contract_id,
                accepter: bob.pubkey,
                collateral: Satoshi::from_sat(500_000),
            },
        )
        .unwrap();
    harness
        .state_machine
        .sign_contract(
            &offer.contract_id,
            ContractSign {
                contract_id: offer.contract_id,
                offerer_signature: harness.sign_message(&alice, offer.contract_id.as_bytes()),
                accepter_signature: harness.sign_message(&bob, offer.contract_id.as_bytes()),
                refund_timeout: Timestamp::from_unix(0),
            },
        )
        .unwrap();
    harness
        .state_machine
        .confirm_funding(&offer.contract_id, 800000, [0u8; 32])
        .unwrap();

    // === Refund due to timeout ===
    let state = harness
        .state_machine
        .refund(&offer.contract_id, RefundReason::Timeout)
        .unwrap();

    assert!(matches!(
        state,
        DlcState::Refunded {
            reason: RefundReason::Timeout,
            ..
        }
    ));
    println!("✓ Contract refunded due to timeout");
}

#[test]
fn test_multi_oracle_quorum() {
    let harness = TestHarness::new();

    let oracle1 = harness.create_oracle("Oracle1");
    let oracle2 = harness.create_oracle("Oracle2");
    let oracle3 = harness.create_oracle("Oracle3");

    // Create 2-of-3 quorum
    let mut quorum = scp_dlc::MultiOracleQuorum::new(
        QuorumConfig::multi(
            2,
            vec![
                OracleInfo {
                    public_key: oracle1.pubkey,
                    name: "Oracle1".to_string(),
                    endpoint: None,
                },
                OracleInfo {
                    public_key: oracle2.pubkey,
                    name: "Oracle2".to_string(),
                    endpoint: None,
                },
                OracleInfo {
                    public_key: oracle3.pubkey,
                    name: "Oracle3".to_string(),
                    endpoint: None,
                },
            ],
        )
        .unwrap(),
    );

    // One oracle attestation is not enough
    quorum
        .add_attestation(harness.create_attestation(&oracle1, 50000))
        .unwrap();
    assert!(quorum.has_quorum().is_none());

    // Two matching attestations reach quorum
    quorum
        .add_attestation(harness.create_attestation(&oracle2, 50000))
        .unwrap();
    assert!(quorum.has_quorum().is_some());

    println!("✓ Multi-oracle quorum verified");
}

#[test]
fn test_invalid_state_transitions() {
    let harness = TestHarness::new();

    let alice = harness.create_party("Alice");
    let oracle = harness.create_oracle("Oracle");

    let offer = ContractOffer {
        contract_id: ContractId::from_data(b"invalid-transition"),
        offerer: alice.pubkey,
        collateral: Satoshi::from_sat(100_000),
        payout_curve: PayoutCurveBuilder::new(Satoshi::from_sat(200_000))
            .binary_winner_takes_all(),
        oracle_info: OracleInfo {
            public_key: oracle.pubkey,
            name: "Oracle".to_string(),
            endpoint: None,
        },
        event_descriptor: "Test".to_string(),
        maturity: Timestamp::from_unix(u64::MAX),
    };

    harness.state_machine.create_offer(offer.clone()).unwrap();

    // Try to settle directly from Offered state (should fail)
    let attestation = harness.create_attestation(&oracle, 50000);
    let result = harness
        .state_machine
        .settle(&offer.contract_id, attestation, 100_000);

    assert!(result.is_err());
    println!("✓ Invalid state transition correctly rejected");
}
