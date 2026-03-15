# SCHEMA INVARIANTS

This document specifies the invariants and constraints for the IPC message schemas between the Rust core and the Python execution engine. These invariants must be maintained to ensure deterministic behavior, reliability, and security of the trading system.

## 1. Required Fields
All fields defined in the `core_types::Event` and `core_types::OmsCommand` structures are required unless explicitly wrapped in an `Option<T>`.
- Missing required fields MUST result in a clean parsing error.
- Null values for non-optional fields MUST result in a clean parsing error.

## 2. Enum Stability
Enum values are serialized using their underlying representation (e.g., `u8`).
- The semantic meaning of an existing enum value MUST NOT change between versions.
- New enum variants MUST be appended to the end of the enum definition.
- Deserializing an unknown enum variant MUST result in a clean parsing error.

## 3. Timestamp Validity
Timestamps (e.g., `ts_src`, `ts_rx`, `ts_proc`) are represented as `u64` microseconds since the UNIX epoch.
- Timestamps MUST be strictly positive (> 0).
- Timestamps MUST represent a reasonable time (e.g., after the system start time).
- `ts_src` MUST NOT be in the future relative to the current system time (allowing for small clock drift tolerances).
- Events older than 5 seconds (5_000_000 microseconds) relative to the current time are considered stale and MUST be rejected.

## 4. Numeric Domain
Numeric fields must adhere to strict domain constraints:
- **Prices:** `price`, `limit_price`, `stop_price`, etc., MUST be strictly positive (> 0.0) and less than or equal to a defined upper bound (e.g., 1000.0 or 100_000.0 depending on the context).
- **Quantities:** `size`, `qty`, `filled_qty`, etc., MUST be strictly positive (> 0).
- **Identifiers:** `symbol_id.0` MUST be strictly positive (> 0). 0 is a reserved invalid identifier.
- **IDs:** String-based IDs like `idempotency_key` MUST NOT be empty strings.

## 5. Backward Compatibility
The system uses MessagePack for serialization.
- The `app_runtime` MUST be able to parse messages sent by a v1 sender even if the sender includes additional (unknown) fields.
- New fields added to the schema MUST be optional or have a default value to maintain compatibility with older clients.

## 6. Malformed Payload Rejection
- Any malformed MessagePack payload MUST result in a clean parsing error (e.g., `Result::Err`).
- Deserializing invalid bytes or types MUST NEVER panic.
- The application MUST enforce `#![deny(clippy::unwrap_in_result)]` to prevent accidental panics during parsing.
