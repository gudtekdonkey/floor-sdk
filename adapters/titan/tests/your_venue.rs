// SPDX-License-Identifier: Apache-2.0
mod common;

use common::SuiteConfig;
use titan_integration_template::your_venue::FloorVenue;
use titan_integration_template::your_venue::{program_id, to_titan};
use solana_pubkey::Pubkey;

#[cfg(debug_assertions)]
#[global_allocator]
static A: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

fn pool() -> Pubkey {
    std::env::var("FLOOR_TITAN_LAUNCH").ok().and_then(|s| s.parse().ok()).unwrap_or_default()
}

fn programs() -> Vec<Pubkey> {
    vec![program_id(), to_titan(&floor_amm_solana::hook::FLOOR_HOOK_ID), to_titan(&floor_amm_solana::keys::TOKEN_2022_PROGRAM_ID)]
}

fn config(test: &str) -> Option<SuiteConfig> {
    let pool = pool();
    if pool == Pubkey::default() {
        println!("SKIP {test}: set FLOOR_TITAN_LAUNCH to a LaunchV2 account");
        return None;
    }
    Some(SuiteConfig { pool, programs: programs() })
}

#[tokio::test]
async fn construction() {
    let Some(c) = config("construction") else { return };
    common::construction::<FloorVenue>(&c).await;
}

#[tokio::test]
async fn zero_input_spot_price() {
    let Some(c) = config("zero_input_spot_price") else { return };
    common::zero_input_spot_price::<FloorVenue>(&c).await;
}

#[tokio::test]
async fn bound_simulation() {
    let Some(c) = config("bound_simulation") else { return };
    common::bound_simulation::<FloorVenue>(&c).await;
}

#[tokio::test]
async fn random_samples() {
    let Some(c) = config("random_samples") else { return };
    common::random_samples::<FloorVenue>(&c).await;
}

#[tokio::test]
async fn monotone() {
    let Some(c) = config("monotone") else { return };
    common::monotone::<FloorVenue>(&c).await;
}

#[tokio::test]
async fn quoting_speed() {
    let Some(c) = config("quoting_speed") else { return };
    common::quoting_speed::<FloorVenue>(&c).await;
}

#[tokio::test]
async fn price_monotone() {
    let Some(c) = config("price_monotone") else { return };
    common::price_monotone::<FloorVenue>(&c).await;
}

#[tokio::test]
async fn mean_value_theorem() {
    let Some(c) = config("mean_value_theorem") else { return };
    common::mean_value_theorem::<FloorVenue>(&c).await;
}
