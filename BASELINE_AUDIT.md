# BASELINE_AUDIT

## Section 1 — Runtime Architecture As-Implemented
- **SlowLoop** (`app_runtime::SlowLoop`): Claims to handle background metrics, market regime, MTF analysis, and tier promotions. Actually reads `EventKind::Tick` and attempts promotion to Tier B/A (counting IBKR subscription budget limits). Checks if `RegimeEngine` says `RiskOff` to terminate immediately. Also calculates MTF via `MtfEngine` but skips `update_weekly_ema()` and `update_daily_resistance()`. Churn calculation runs but no demotion logic is triggered in `WatchlistEngine`. Handles `Heartbeat`.
- **FastLoop** (`app_runtime::FastLoop`): Evaluates entry rules dynamically. It loads zero-allocation shared state, checks halting, checks entry gates (TapeEngine), checks slippage, and constructs OMS orders. Claims to pass `predicted_slippage` to CalibrationLogger, but actually passes `0.0`. Supports `paper_mode` feature to log locally instead of executing. Calculates PDT guard violation with `today_ordinal = (event.ts_src / 1_000_000 / 86400) as u32;` (which is UTC, not US Eastern Time).
- **Metrics/Observability Loop** (`metrics_rx` loop in `app_runtime::run`): Processes raw log decisions and latency. Checks P95 SLA limit. If it hard fails (>5 seconds >25ms), it forces `monitor_only` mode via RiskState.
- **Risk Task Loop** (`risk_tx` loop in `app_runtime::run`): Listens for Fill events, updates `RiskState` PnL, and automatically triggers partial exits on specific profit targets.
- **OMS Task Loop** (`oms_tx` loop in `app_runtime::run`): Manages orders (`NewOrder`, `Cancel`). Check timeouts. On shutdown, explicitly tries to map and cancel live orders.

## Section 2 — Placeholder Inventory
rust/crates/tape_engine/src/lib.rs:156 | logic | medium | `// For now, we will pass 0 or compute it if needed in check_entry.`
rust/crates/tape_engine/src/lib.rs:159 | logic | low | `// Let's compute it from ts_src for now, assuming ts_src is unix epoch micros`
rust/crates/tape_engine/src/lib.rs:218 | implementation | low | `state.last_trade_price = tick.price; // Simplified for now`
rust/crates/tape_engine/src/lib.rs:981 | test | low | `// Placeholder: this test validates the concept compiles.`
rust/crates/risk_engine/src/lib.rs:172 | integration | medium | `// Assuming Blocklist for now or adding PDT in core_types later.`
rust/crates/risk_engine/src/lib.rs:228 | architecture | medium | `// For now, we assume local state (persistence) is the source of truth for PnL,`
rust/crates/bridge_rx/src/lib.rs:237 | optimization | low | `// For now, drain is fine for small batches.`
rust/crates/app_runtime/src/lib.rs:197 | architecture | high | `// TODO: A dedicated Rust→Python cancel command channel is required for production.`
rust/crates/app_runtime/src/lib.rs:255 | logic | medium | `// For now, log actual slippage to calibration_logger`
rust/crates/app_runtime/src/lib.rs:295 | logic | medium | `// For now: log + set symbol as halted in a local set.`
rust/crates/event_bus/src/lib.rs:51 | architecture | medium | `// For now, we assume Event covers market data, but we need a command channel for orders.`
rust/bins/replayer/src/main.rs:84 | test | medium | `// and capture the result. Placeholder for now:`
python/rps_bridge/encoder.py:12 | implementation | high | `pass` inside `encode_flatbuffers`
python/rps_bridge/uds_sender.py:28 | implementation | medium | `# Using msgpack for now as per guidance`

## Section 3 — Silent Gaps Verification
Confirmed as-documented:
(a) `update_weekly_ema()` is defined in `MtfEngine` but is never called from `SlowLoop`.
(b) `update_daily_resistance()` is defined in `MtfEngine` but is never called from `SlowLoop`.
(c) CalibrationLogger predicted_slippage is always 0.0 (`calib_logger.record(..., 0.0, actual_slip)` in `FastLoop`).
(d) PDT guard uses UTC today_ordinal `(event.ts_src / 1_000_000 / 86400) as u32;` instead of US Eastern Time.
(e) WatchlistEngine `demote()` method exists, but `wl.demote()` is never called in `app_runtime`.
(f) `encode_flatbuffers()` in `python/rps_bridge/encoder.py` just executes `pass`.

## Section 4 — Test Inventory
Cargo test results for all 61 tests:
```
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 1 test
test tests::test_context_computation ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 7 tests
test locale::tests::test_all_reasons_have_keys ... ok
test tests::print_layout ... ok
test time_buffer::tests::test_capacity_overflow ... ok
test time_buffer::tests::test_empty_behavior ... ok
test time_buffer::tests::test_time_eviction ... ok
test config::tests::test_config_parse ... ok
test config::tests::test_load_default_config_file ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 3 tests
test tests::test_handle_fill ... ok
test tests::test_timeouts ... ok
test tests::test_place_order_idempotency ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 2 tests
test tests::test_mtf_evaluation ... ok
test tests::test_mtf_fail ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 1 test
test tests::test_transitions ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 28 tests
test exposure::tests::test_max_2_strict ... ok
test exposure::tests::test_correlation_restriction ... ok
test exposure::tests::test_single_position_default ... ok
test exposure::tests::test_sector_restriction ... ok
test guards::tests::test_flicker_guard ... ok
test guards::tests::test_imbalance_guard ... ok
test guards::tests::test_l2_vacuum ... ok
test guards::tests::test_spread_guard ... ok
test guards::tests::test_stale_guard ... ok
test guards::tests::test_slippage_guard ... ok
test guards::tests::test_ttl_hysteresis ... ok
test pdt::tests::test_pdt_counting ... ok
test pdt::tests::test_pdt_expiry ... ok
test sizing::tests::test_budget_cap ... ok
test session::tests::test_session_states ... ok
test sizing::tests::test_liquidity_cap ... ok
test sizing::tests::test_max_position_pct ... ok
test sizing::tests::test_min_stop_distance ... ok
test sizing::tests::test_pricing_model_expected_net_negative_high_fees ... ok
test sizing::tests::test_pricing_model_expected_net_positive ... ok
test sizing::tests::test_pricing_model_fees ... ok
test sizing::tests::test_risk_constrained_size ... ok
test tests::test_corporate_action_block ... ok
test tests::test_manual_block ... ok
test tests::test_monitor_only ... ok
test tests::test_max_daily_loss ... ok
test tests::test_risk_ladder ... ok
test tests::test_persistence ... ok
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 14 tests
test tests::stress_luld_halt_new_entry_blocked ... ok
test tests::integration_full_decision_chain_happy_path ... ok
test tests::integration_full_decision_chain_each_gate_blocks ... ok
test tests::stress_reconnect_during_open_position ... ok
test tests::stress_regime_change_riskoff_during_position ... ok
test tests::stress_spread_widening_triggers_guard ... ok
test tests::stress_stale_quotes_triggers_guard ... ok
test tests::test_guard_failure ... ok
test tests::test_mtf_veto ... ok
test tests::test_full_pass ... ok
test tests::test_regime_requirement ... ok
test tests::test_tapescore_low ... ok
test tests::test_tier_a_requirement ... ok
test tests::stress_burst_ticks_no_alloc ... ok
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 5 tests
test tests::test_cold_start_controller ... ok
test tests::test_demotion_path ... ok
test tests::test_promotion_path ... ok
test tests::test_surge_override ... ok
test tests::test_subscription_limits ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 3 tests
test crates/risk_engine/src/sizing.rs - sizing::PricingModel::expected_net (line 155) ... ok
test crates/risk_engine/src/session.rs - session::SessionGuard::entry_allowed (line 65) ... ok
test crates/risk_engine/src/blocklist.rs - blocklist::Blocklist::is_blocked (line 60) ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## Section 5 — Hardcoded Values & Security Defaults
- **Sockets**: `/var/run/rps/rps_uds.sock` in `app_runtime`, `ibkr_client.py`, and `uds_sender.py`.
- **Paths**: `/var/run/rps/risk_state.json`, `/var/run/rps/rps_commands.sock`.
- **HTTP Dashboard**: Listens on `0.0.0.0:8080` (`rpsd/src/main.rs`).
- **WebSocket**: Hardcoded `ws://${location.host}/ws` in `dashboard/index.html`.
- **CDN URLs**: React / ReactDOM / Babel load externally from `https://cdn.jsdelivr.net/...` in `dashboard/index.html`.

## Section 6 — Execution Map
1. **Market Data Intake** (Python `ibkr_client.py`) -> `uds_sender.py` (msgpack encoder) -> `/var/run/rps/rps_uds.sock` (Unix Socket) -> `BridgeRxTask` (Rust) (Functional)
2. **Event Router** (`app_runtime::run`) -> `FastLoop`, `SlowLoop`, `RiskTask`, `OmsTask` (Functional)
3. **SlowLoop / Analytics**: Regime analysis (Functional), MTF Analysis (Missing Weekly EMA / Daily Res updates), Churn detection (Functional but no demotion logic triggered).
4. **FastLoop / Gate Processing**: `WatchlistSnapshot` read (Functional) -> `TapeEngine::on_event` -> `evaluate_entry_logic` 12-gates (Functional) -> Check Slippage Guard (Functional) -> Prepare Order (Functional).
5. **OMS/Risk Execution**: Order mapped to Python command channel (Broken: Rust->Python cancel/command channel explicitly marked `TODO`, orders simulated if `paper_mode` enabled or dropped if not setup). PDT check uses UTC ordinal instead of NY time (Broken).
6. **Metrics/Calibration**: Decisions logged, latency tracked. Slippage logged but with hardcoded predicted=0.0 (Placeholder).

## Section 7 — 'For Now' Comment Inventory
- `rust/crates/tape_engine/src/lib.rs:156` | `// For now, we will pass 0 or compute it if needed in check_entry.` | M
- `rust/crates/tape_engine/src/lib.rs:159` | `// Let's compute it from ts_src for now, assuming ts_src is unix epoch micros` | S
- `rust/crates/tape_engine/src/lib.rs:218` | `state.last_trade_price = tick.price; // Simplified for now` | S
- `rust/crates/tape_engine/src/lib.rs:981` | `// Placeholder: this test validates the concept compiles.` | S
- `rust/crates/risk_engine/src/lib.rs:172` | `// Assuming Blocklist for now or adding PDT in core_types later.` | M
- `rust/crates/risk_engine/src/lib.rs:228` | `// For now, we assume local state (persistence) is the source of truth for PnL,` | M
- `rust/crates/bridge_rx/src/lib.rs:237` | `// For now, drain is fine for small batches.` | S
- `rust/crates/app_runtime/src/lib.rs:197` | `// TODO: A dedicated Rust→Python cancel command channel is required for production.` | L
- `rust/crates/app_runtime/src/lib.rs:255` | `// For now, log actual slippage to calibration_logger` | M
- `rust/crates/app_runtime/src/lib.rs:295` | `// For now: log + set symbol as halted in a local set.` | S
- `rust/crates/event_bus/src/lib.rs:51` | `// For now, we assume Event covers market data, but we need a command channel for orders.` | L
- `rust/bins/replayer/src/main.rs:84` | `// and capture the result. Placeholder for now:` | M
- `python/rps_bridge/encoder.py:12` | `pass` inside `encode_flatbuffers` | M
- `python/rps_bridge/uds_sender.py:28` | `# Using msgpack for now as per guidance` | M
