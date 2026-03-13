# LIVE_BLOCKERS

| Unique ID | File:Line | Classification | Phase | Justification |
| --- | --- | --- | --- | --- |
| I-01 | `rust/crates/app_runtime/src/lib.rs:197` | Resolved | Execution/Rust | Missing a dedicated command channel to send explicit cancellations from Rust to the Python execution engine, meaning canceled orders remain live on IBKR. |
| I-02 | `rust/crates/mtf_engine/src/lib.rs:53` | Resolved | Analytics/Rust | The `update_weekly_ema` (and `update_daily_resistance`) functions are never called by the SlowLoop, leaving MTF evaluations incomplete or stuck in default states. |
| I-03 | `rust/crates/app_runtime/src/lib.rs:401` | Resolved | Risk/Rust | The PDT guard uses a UTC-based `today_ordinal` calculation instead of US Eastern Time, which breaks the T+1 settlement logic and will miscount day trades. |
| I-04 | `dashboard/index.html:145` | Resolved | UI/Dashboard | The dashboard attempts to render via ReactDOM but the backend only provides a static HTML shell; it is a fake/placeholder dashboard that does not properly initialize React components from a build system. |
| I-05 | `rust/bins/replayer/src/main.rs:84` | Resolved | Testing/Replayer | The L1 replayer logic contains a 'Placeholder for now' comment instead of actually executing the decision pipeline to generate or compare golden files. |
| I-06 | `python/rps_bridge/encoder.py:12` | High | Bridge/Python | The `encode_flatbuffers` function is just a `pass` block, meaning flatbuffer serialization is unimplemented and standardizing on high-throughput parsing is blocked. |
| I-07 | `rust/crates/risk_engine/src/lib.rs:228` | High | Risk/Rust | The system assumes local state persistence is the source of truth for PnL without continuously syncing and reconciling actual execution fills from the broker. |
| I-08 | `rust/crates/app_runtime/src/lib.rs:261` | High | Observability/Rust | The FastLoop passes a hardcoded `0.0` for predicted slippage into the `CalibrationLogger`, entirely defeating the α/β recalibration mechanism. |
| I-09 | `rust/crates/watchlist_engine/src/lib.rs:266` | High | Analytics/Rust | The `WatchlistEngine::demote` method exists but is never invoked by the `SlowLoop` during churn detection, resulting in permanently elevated symbol tiers. |
| I-10 | `rust/bins/rpsd/src/main.rs:62` | Medium | Infrastructure/Rust | The web server router setup contains duplicated route configurations, cluttering the API endpoints or accidentally shadowing valid routes. |
| I-11 | `rust/crates/tape_engine/src/lib.rs:505` | Medium | Infrastructure/Rust | A rogue `println!` statement in the hot path of the decision logic causes unnecessary blocking I/O and degrades zero-allocation performance guarantees. |
| I-12 | `rust/crates/app_runtime/src/lib.rs:24` | Medium | Infrastructure/Rust | Hardcoded absolute paths like `/var/run/rps/risk_state.json` break local development environments and enforce strict OS dependencies. |
| I-13 | `rust/bins/rpsd/src/main.rs:63` | Medium | Infrastructure/Rust | The web server explicitly binds to `0.0.0.0:8080`, exposing the internal administration dashboard/websocket to all network interfaces without authentication. |
| I-14 | `rust/crates/app_runtime/Cargo.toml:26` | Medium | Infrastructure/Rust | There is a duplicate `anyhow = "1"` dependency entry causing a compilation error with `cargo test --workspace`. |
| I-15 | `dashboard/index.html:64` | Medium | UI/Dashboard | The dashboard's WebSocket connection lacks any authentication layer, leaving the system state broadly accessible. |
| I-16 | `python/rps_bridge/ibkr_client.py:120` | Medium | Infrastructure/Python | The system demands root privileges to run because of the hardcoded `/var/run/rps` socket path, violating principle of least privilege. |
| I-17 | `README.md:1` | Medium | Documentation | The top-level README is out of sync with the codebase's current implemented state or completely missing key execution directions. |
| I-18 | `rust/crates/risk_engine/src/lib.rs:60` | Low | Risk/Rust | The blocklist loads from a hardcoded `configs/blocklist.toml` example path rather than taking a dynamic environment-driven parameter. |
| I-19 | `rust/crates/tape_engine/src/lib.rs:381` | Low | Risk/Rust | Using `.expect()` during symbol state retrieval introduces a potential thread panic in the FastLoop instead of safely returning a Result. |
| I-20 | `rust/Cargo.toml:22` | Low | Infrastructure/Rust | Workspace dependency versions are declared inconsistently between the root `Cargo.toml` and crate-level `Cargo.toml`. |
| I-21 | `rust/crates/tape_engine/src/lib.rs:1` | Low | Documentation | While `#![deny(clippy::unwrap_in_result)]` is set, there is no strict `#![warn(missing_docs)]` enforcement for core trading logic modules. |
| I-22 | `rust/crates/tape_engine/src/lib.rs:159` | Low | Analytics/Rust | The code contains 'for now' comments calculating ordinal times or simplifying state that need refactoring before finalizing 1.0 architecture. |
| I-23 | `dashboard/index.html:9` | Low | UI/Dashboard | The UI relies on in-browser Babel compilation via a CDN for React JSX, slowing down dashboard load times and risking failure on internet outages. |

**TOTAL:** Critical 0 | High 4 | Medium 8 | Low 6 | Resolved 5
