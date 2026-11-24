//! Your venue's swap-route test — the same end-to-end suite the example passes,
//! run against `YourVenue`. Red once you've implemented YourVenue and pointed the
//! config below at a real pool + program (with SOLANA_RPC_URL set and the route
//! program built); SKIPs cleanly until then.

mod common;

use common::{RouteConfig, run_swap_route};
use solana_pubkey::Pubkey;
use titan_integration_template::your_venue::YourVenue;

fn pool() -> Pubkey {
    std::env::var("FLOOR_TITAN_LAUNCH").ok().and_then(|s| s.parse().ok()).unwrap_or_default()
}

fn venue_programs() -> Vec<Pubkey> {
    use titan_integration_template::your_venue::{program_id, to_titan};
    vec![program_id(), to_titan(&floor_amm_solana::hook::FLOOR_HOOK_ID), to_titan(&floor_amm_solana::keys::TOKEN_2022_PROGRAM_ID)]
}

#[tokio::test]
async fn swap_route_both_directions() {
    run_swap_route::<YourVenue>(RouteConfig {
        pool: pool(),
        venue_programs: venue_programs(),
    })
    .await;
}
