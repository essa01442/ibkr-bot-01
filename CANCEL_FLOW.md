# Cancel Flow Architecture

## Overview
This document describes the state machine and flow architecture for canceling an order in the Robust Penny Scalper system. It guarantees that orders in a `PendingCancel` or `CancelSent` state strictly transition to terminal states via explicit broker responses (`CancelAck` or `CancelReject`) or an explicit `CancelTimeout` escalation, ensuring no order is "silently lost."

## State Machine Diagram
```mermaid
stateDiagram-v2
    [*] --> Pending
    Pending --> Live: Broker Ack (Open)
    Pending --> Filled: Partial/Full Fill

    Pending --> PendingCancel: Cancel Request (from FastLoop/OMS)
    Live --> PendingCancel: Cancel Request

    PendingCancel --> CancelSent: Bridge Dispatch

    CancelSent --> Cancelled: EventKind::CancelAck
    CancelSent --> CancelRejected: EventKind::CancelReject
    CancelSent --> CancelTimeout: OMS check_cancel_timeouts (>= 5000ms)

    CancelTimeout --> Cancelled: Late Ack (EventKind::CancelAck)
```

## State Definitions

| State | Description | Is Terminal? |
|-------|-------------|-------------|
| **PendingCancel** | Order is flagged for cancellation locally; waiting to be picked up and routed over IPC to Python bridge. | No |
| **CancelSent** | The `CancelRequest` has been emitted over IPC. The system is actively awaiting the broker's response. | No |
| **Cancelled** | Broker confirmed the cancellation (`CancelAck` received), or local timeout cleanly finalized a stale connection state. | Yes |
| **CancelRejected** | Broker explicitly rejected the cancellation (e.g., already filled, already cancelled, or invalid ID). Logs an error and alerts. | Yes |
| **CancelTimeout** | Escalar state reached if `now - cancel_requested_time > config.execution.cancel_timeout_ms` (Default: 5000ms). Triggers system alerts. | Yes |

## IPC and Bridge Flow
1. FastLoop / RiskTask submits `OmsCommand::CancelOrder(CancelRequest)`.
2. OMS receives it, marks `OrderStatus::PendingCancel`.
3. OMS immediately translates this to Python Bridge via `CancelSent` mark, emitting via `bridge_cmd_tx` (or equivalent sync channel).
4. Python Bridge (`ibkr_client.py`) receives IPC, executes `ib.cancelOrder()`.
5. Broker responds. Python Bridge constructs `CancelAckData` or `CancelRejectData` payload.
6. Event arrives on UDS, `BridgeRxTask` parses into `EventKind::CancelAck` or `EventKind::CancelReject`.
7. Event Router sends to `oms_tx`.
8. OMS state resolves terminal outcome (`handle_cancel_ack` or `handle_cancel_reject`).

## Edge Case Handlings
* **Duplicate Cancels**: Successive `cancel_order` calls on an ID already in `PendingCancel` or `CancelSent` return `Ok(())` silently and are effectively ignored to prevent double-transmission overhead.
* **Already Filled**: Trying to cancel an order sitting in `Filled` yields an immediate `Err("Order in terminal state, cannot cancel")`.
* **Late Acks**: If a cancel request reaches `CancelTimeout` and then the broker's Ack arrives, the state transitions from `CancelTimeout -> Cancelled` gracefully to ensure strict local/remote alignment.
