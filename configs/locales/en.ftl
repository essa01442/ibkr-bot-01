# Robust Penny Scalper v7.0 — English Locale

## Reject Reasons
reject-blocklist = Symbol blocked
reject-corporate-action-block = Corporate action — blocked
reject-price-range = Outside target price range
reject-liquidity = Insufficient liquidity
reject-regime = Regime not Normal
reject-daily-context = No daily catalyst
reject-mtf-veto = MTF context veto
reject-anti-chase = Anti-chase — rapid run-up
reject-guard-spread = Spread too wide
reject-guard-imbalance = Order book imbalance
reject-guard-stale = Stale quotes detected
reject-guard-slippage = Expected slippage too high
reject-guard-l2-vacuum = L2 depth vacuum
reject-guard-flicker = Quote flickering
reject-tape-score-low = TapeScore below threshold
reject-net-negative = Negative expected net
reject-exposure = Exposure/correlation limit
reject-tape-reversal = Tape reversal detected
reject-monitor-only = System in Monitor Only mode
reject-max-daily-loss = Daily loss limit reached
reject-pdt-violation = PDT/settlement violation

## Regime States
regime-normal = Normal
regime-caution = Caution
regime-risk-off = Risk-Off

## OMS States
oms-idle = Idle
oms-active = Active
oms-investigate = Investigate — no new orders

## Data Quality
data-quality-ok = Data OK
data-quality-degraded = Data Degraded — Monitor Only

## Cold Start States
cold-start-cold = Cold Start
cold-start-warm = Warm Active
cold-start-full = Full Active

## Session States
session-closed = Market Closed
session-pre-market = Pre-Market
session-open-volatility = Open Volatility Window — waiting
session-trading = Trading Hours
session-close-volatility = Closing Window — no new entries
session-after-hours = After Hours

## Alerts
alert-data-api-down = ALERT: IBKR connection lost
alert-heartbeat-missing = ALERT: No heartbeat — { $duration_secs }s
alert-sla-breach = ALERT: SLA breach — P95 = { $p95_micros }µs
alert-daily-loss-limit = ALERT: Daily loss limit reached (${ $loss_usd })
alert-loss-ladder-activated = ALERT: Loss ladder level { $level } activated
alert-order-anomaly = ALERT: Order anomaly — order { $order_id }
alert-ibkr-subs-high = ALERT: IBKR subscriptions { $current }/{ $limit }
alert-mtf-reject-high = ALERT: MTF reject rate high ({ $rate_pct }%)

## Guard Names
guard-spread = Spread Guard
guard-imbalance = Imbalance Guard
guard-stale = Stale Quote Guard
guard-slippage = Slippage Gate
guard-l2-vacuum = L2 Vacuum Guard
guard-flicker = Quote Flicker Guard

## Loss Attribution
loss-entry-model = Entry Model
loss-context = Context (Daily/MTF)
loss-guards = Structural Guards
loss-execution = Execution/Slippage
loss-risk = Risk Management
loss-data = Data Quality

## Exit Reasons
exit-target = Target Reached
exit-stop = Stop Loss
exit-manual = Manual Close
exit-luld-halt = Emergency — LULD Halt
exit-regime-change = Regime Change
exit-tape-reversal = Tape Reversal
exit-session-close = Session Close
exit-unknown = Unknown