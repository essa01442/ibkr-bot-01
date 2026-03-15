1.  **Scenario 1: Market data intake**
    -   *Expected:* Data from the IBKR stub (via Python bridge) flows through to the Rust system correctly.
    -   *Observed:* TBD
    -   *Result:* TBD
2.  **Scenario 2: Risk gate activation**
    -   *Expected:* When a risk limit is triggered, orders are blocked, and an alert is logged.
    -   *Observed:* TBD
    -   *Result:* TBD
3.  **Scenario 3: MTF gate with real data**
    -   *Expected:* MTF gate evaluates real, non-zero data and accurately blocks or allows the trade.
    -   *Observed:* TBD
    -   *Result:* TBD
4.  **Scenario 4: Order submission**
    -   *Expected:* An order is successfully placed via a paper account, and a confirmation is received.
    -   *Observed:* TBD
    -   *Result:* TBD
5.  **Scenario 5: Cancel flow roundtrip**
    -   *Expected:* An order is placed, then successfully cancelled, with a cancel acknowledgment received.
    -   *Observed:* TBD
    -   *Result:* TBD
6.  **Scenario 6: Fill reflection + PnL**
    -   *Expected:* A fill is received; the `trade_accounting` module computes the PnL, which is then correctly displayed.
    -   *Observed:* TBD
    -   *Result:* TBD
7.  **Scenario 7: Dashboard visibility**
    -   *Expected:* All fields on the dashboard show actual, live data during an active session.
    -   *Observed:* TBD
    -   *Result:* TBD
8.  **Scenario 8: Stale data degradation**
    -   *Expected:* If the data feed is paused, the system gracefully degrades to a NEUTRAL state.
    -   *Observed:* TBD
    -   *Result:* TBD
9.  **Scenario 9: Emergency exit**
    -   *Expected:* Simulating an LULD halt causes the system to halt all activity and log the event.
    -   *Observed:* TBD
    -   *Result:* TBD
