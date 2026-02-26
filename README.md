# Robust Penny Scalper v7.0 FINAL

## Overview

This repository contains the source code for the "Robust Penny Scalper" system, designed for high-frequency scalping of penny stocks on IBKR.

### Architectural Principles

*   **Production-Ready Rust**: Built with reliability and performance as top priorities.
*   **Deterministic Design**: All logic must be predictable and testable.
*   **Zero-Allocation Hot Path**: Critical paths (FastLoop) must not allocate memory after initialization.
*   **Locked Decision Order**: Strict adherence to the decision pipeline.
*   **Rust for Decisions**: Python is strictly a transport layer. Rust owns all logic.

### System Summary

*   **Broker**: IBKR
*   **Bridge**: Python (ib_insync) – transport only
*   **Core**: Rust (Tokio)
*   **IPC**: Unix Domain Socket
*   **Serialization**: MessagePack (dev) → FlatBuffers (prod)
*   **SLA**: 10ms internal | 100ms hard max E2E

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

### Requirements

*   Rust (latest stable)
*   Python 3.10+
*   FlatBuffers compiler (`flatc`)
