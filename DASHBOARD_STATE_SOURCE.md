# Dashboard State Source Documentation

## Overview
This document specifies the internal system sources for every field populated in the `SystemSnapshot` exposed to the frontend Dashboard WebSocket.

Currently, the system uses **Option B (Fallback Status)** for local development and safety. The fields are explicitly marked as `NOT_WIRED` to prevent synthetic or dummy data from masquerading as actual live system telemetry.

## SystemSnapshot Fields

| Field Name | Expected Source Component | Current Implementation Status |
|------------|---------------------------|-------------------------------|
| `ts_ms` | `std::time::SystemTime::now()` | **Live:** Returns system UNIX time. |
| `regime` | `RegimeEngine` (SlowLoop) | **NOT_WIRED**: Hardcoded to `"NOT_WIRED"`. |
| `data_quality` | `BridgeRxTask` | **NOT_WIRED**: Hardcoded to `"NOT_WIRED"`. |
| `monitor_only` | `RiskState.monitor_only` | **Fallback**: Forced to `true` to ensure visual safety block. |
| `session_state` | `SessionGuard` | **NOT_WIRED**: Hardcoded to `"NOT_WIRED"`. |
| `daily_pnl_usd` | `RiskState.daily_realized_pnl` | **Fallback**: Forced to `f64::NAN` to explicitly show invalid data. |
| `loss_ladder_level` | `RiskState` / Ladder Logic | **NOT_WIRED**: Hardcoded to `0`. |
| `max_daily_loss_remaining` | `RiskState.max_daily_loss_usd` | **Fallback**: Forced to `f64::NAN`. |
| `open_positions` | `RiskState.positions.len()` | **NOT_WIRED**: Hardcoded to `0`. |
| `oms_state` | `OrderManagementSystem` | **NOT_WIRED**: Hardcoded to `"NOT_WIRED"`. |
| `p95_latency_us` | `MetricsObservability.LatencyTracker` | **NOT_WIRED**: Hardcoded to `0`. |
| `ibkr_subscription_count` | `WatchlistEngine` | **NOT_WIRED**: Hardcoded to `0`. |
| `ibkr_subscription_budget` | `AppConfig.ibkr.subscription_budget`| **NOT_WIRED**: Hardcoded to `0`. |
| `recent_rejects` | `MetricsObservability.MetricsCollector` | **Fallback**: Hardcoded vector `["SYSTEM_NOT_WIRED"]`. |
| `recent_alerts` | `MetricsObservability.AlertManager` | **Fallback**: Hardcoded vector `["DASHBOARD_NOT_WIRED"]`. |
| `is_synthetic` | WebSocket Broadcast Emitter (`main.rs`) | **Live**: Set to `true` currently. Triggers an `assert!` block before WebSocket dispatch. |

## Assertions & Safety
To completely eliminate the risk of a "Fake Normal" dashboard (e.g., displaying $0.00 PnL and Normal regime while disconnected), the `handle_socket` function in `dashboard.rs` asserts:
```rust
debug_assert!(!snapshot.is_synthetic, "FATAL: Synthetic snapshot passed to live WebSocket!");
```
This forces the application to panic and drop the connection in debug mode rather than mislead the trader if a synthetic event attempts to reach the browser.
