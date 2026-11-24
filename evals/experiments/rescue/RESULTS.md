# Test A: results (Nov 23, 2026)

Run as `PLAN.md` fixed it: haiku, `memory` layer, 13 cases, 3 repeats, 39 runs, none with an
error. `data/results.jsonl` holds the four arms of every run (`python3 analyze.py
data/results.jsonl`). Cost of the whole batch: $0.26 (the CLI's estimate).

**The trigger never fired.** The project's tests passed after all 39 first attempts, so the
three arms delivered the first attempt every time: same result, no extra tokens.

| | passed | Δ with base | 95 % interval | rescues | breaks | invalid |
|---|---|---|---|---|---|---|
| base (one AI) | 33/39 | | | | | |
| same session told the failure | 33/39 | +0 | [0, 0] | 0 | 0 | 0 |
| new session told the failure | 33/39 | +0 | [0, 0] | 0 | 0 | 0 |
| second attempt (control) | 33/39 | +0 | [0, 0] | 0 | 0 | 0 |

By case (base = every arm):

| case | passed | tests fired |
|---|---|---|
| attachment-download | 3/3 | 0 |
| bakery-inscription | 3/3 | 0 |
| billing-cents | 3/3 | 0 |
| business-days | 3/3 | 0 |
| easy-greeting | 3/3 | 0 |
| inventory-expiry | 2/3 | 0 |
| invoice-rounding | 2/3 | 0 |
| library-holiday-fine | 3/3 | 0 |
| parse-duration | 1/3 | 0 |
| rate-limit-bugs | 3/3 | 0 |
| receipt-api-key | 3/3 | 0 |
| sales-summary-rules | 3/3 | 0 |
| shipping-refactor | 1/3 | 0 |

**Where the 6 failures were.** All six passed the project's tests, the agent's own new ones
included, and failed what those tests do not cover: a bare number taken as minutes
(`parse-duration`, twice), the free-shipping threshold still checked twice
(`shipping-refactor`, twice), `invoice_total([])` raising (`invoice-rounding`), and `release_expired()`
returning the wrong count (`inventory-expiry`). The agent runs the tests itself before it stops; a failure
it can see, it already fixes.

**Decision (as fixed):** the trigger fired in 0 % of first attempts (< 5 %), so «Verificar y
devolver el fallo» is not built as a default. The bottleneck is the tests, not who runs them.
Sonnet was not run: haiku left nothing open.
