# DOCS_SYNC

## Task A — README File Tree Sync (I-17)
Updated `README.md` to precisely reflect the implemented file tree.
* **File Modified**: `README.md`
* **Before**:
```markdown
### Folder Structure

*   `configs/`: Configuration files (TOML) and locales.
*   `docs/`: Documentation and specifications.
*   `proto/`: Protocol Buffers / FlatBuffers schemas.
*   `python/`: Python bridge code (`rps_bridge`).
*   `rust/`: Rust workspace containing the core logic.
    *   `crates/`: Modularized crates for specific domains.
        *   `app_runtime`: Application wiring and task management.
        *   `bridge_rx`: Receiving and decoding data from the bridge.
        *   `core_types`: Shared domain types and error definitions.
        *   `event_bus`: Communication channels between tasks.
        *   `execution_engine`: Order Management System (OMS).
        *   `metrics_observability`: Logging, metrics, and alerting.
        *   `risk_engine`: Risk checks and limits.
        *   `tape_engine`: FastLoop logic (Tape reading, guards).
        *   `watchlist_engine`: SlowLoop logic (Candidate selection).
    *   `bins/`: Executable binaries (`rpsd`).
*   `logs/`: Runtime logs.
```
* **After**:
```markdown
### Folder Structure

*   `configs/`: Configuration files (TOML) and locales.
*   `dashboard/`: Frontend React application for real-time monitoring.
*   `docs/`: Documentation and specifications.
*   `logs/`: Runtime logs.
*   `proto/`: Protocol Buffers / FlatBuffers schemas.
*   `python/`: Python bridge code (`rps_bridge`).
*   `rust/`: Rust workspace containing the core logic.
    *   `bins/`: Executable binaries (`replayer`, `rpsd`).
    *   `crates/`: Modularized crates for specific domains.
        *   `app_runtime`: Application wiring and task management.
        *   `bridge_rx`: Receiving and decoding data from the bridge.
        *   `context_engine`: Daily context and volume analysis.
        *   `core_types`: Shared domain types and error definitions.
        *   `event_bus`: Communication channels between tasks.
        *   `execution_engine`: Order Management System (OMS).
        *   `metrics_observability`: Logging, metrics, and alerting.
        *   `mtf_engine`: Multi-Timeframe Analysis.
        *   `regime_engine`: Market regime and overall trend tracking.
        *   `risk_engine`: Risk checks, guards, and limits.
        *   `tape_engine`: FastLoop logic (Tape reading, guards).
        *   `watchlist_engine`: SlowLoop logic (Candidate selection).
```

## Task B — Missing Directories (I-21)
* **Chosen Option**: Option 1: Create placeholder directories with a `README.md` inside each.
* **Justification**: The `docs/` and `logs/` directories represent structural intent for standard runtime and developer workflows. Deleting their references would mean backtracking on the desired architecture. Furthermore, files like `BASELINE_AUDIT.md`, `LIVE_BLOCKERS.md`, and `DOCS_SYNC.md` will likely migrate to `docs/` in the near future. Creating the folders resolves the mismatch without losing the architectural mapping.
* **Files Modified / Created**:
    * `docs/README.md` (Created)
    * `logs/README.md` (Created)
* **Before**: Directories did not exist.
* **After**:
    * `docs/README.md` contains:
```markdown
# Docs

Placeholder for documentation and specifications.
```
    * `logs/README.md` contains:
```markdown
# Logs

Placeholder for runtime logs.
```

## Task C — Version Label Consistency (I-20)
* **Canonical Version String**: `v7.0`
* **File Modified**: `README.md`
* **Before**:
```markdown
# Robust Penny Scalper v7.0
```
* **After**:
```markdown
# Robust Penny Scalper v7.0
```
