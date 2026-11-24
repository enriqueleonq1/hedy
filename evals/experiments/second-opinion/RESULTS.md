# Test B: results (Nov 23, 2026)

Run as `PLAN.md` fixed it: 40 diffs (20 bad: 6 from test A's runs and 14 planted; 20 good,
all from haiku), haiku and Sonnet as reviewers, 3 repeats, 240 reviews, none with an error.
Every high or medium finding was labelled by hand (141) and by a blind Opus judge.
`data/` holds the material, the reviews and both sets of labels
(`python3 analyze.py data/material.json data/reviews.jsonl data/labels-manual.json data/labels-opus.json`).
Cost: $5.23 for the reviews and their verifications, $1.86 for the judge (the CLI's estimates).

## All 40 diffs

Hand labels; intervals by bootstrap over cases.

| arm | detects (bad) | 95 % | false alarms / good | 95 % | precision | $ / diff | $ / detection | rule |
|---|---|---|---|---|---|---|---|---|
| same (haiku) | 78 % | [62, 100] | 0.17 | [0.00, 0.40] | 78 % | 0.0035 | 0.009 | ✓ |
| same + verify | 78 % | [62, 100] | 0.17 | [0.00, 0.40] | 78 % | 0.0051 | 0.013 | ✓ |
| other (Sonnet) | 72 % | [52, 95] | 0.05 | [0.00, 0.18] | 91 % | 0.0282 | 0.079 | ✓ |
| other + verify | 72 % | [52, 95] | 0.05 | [0.00, 0.18] | 91 % | 0.0385 | 0.108 | ✓ |
| same × 2 (2 calls) | 82 % | [67, 100] | 0.33 | [0.00, 0.79] | 78 % | 0.0070 | 0.017 | ✗ |
| same + other (2 calls) | 78 % | [62, 100] | 0.22 | [0.00, 0.52] | 83 % | 0.0317 | 0.081 | ✓ |

With the judge's labels the detections are the same, but haiku's false alarms rise to 0.27
per good diff (the rule fails) and Sonnet's stay at 0.05. The two sets agree on 126 of 141
findings (89 %, Cohen's kappa 0.79). Nearly all of the 15 disagreements are findings I took as
real and the judge as false: a missing length limit for the new free-text field, and
confirming a reservation after `release_expired()` raising the wrong error. Both are gaps
against the request that no hidden test covers, and the reference solutions have them too.

## What decides it: planted errors against real ones

| | haiku | Sonnet |
|---|---|---|
| planted errors detected (14 diffs × 3) | 42/42 | 41/42 |
| real failures of haiku detected (6 diffs × 3) | **5/18** | **2/18** |

The planted errors are one clean edit on top of a correct solution, and both models saw
nearly all of them. Haiku's own failures were another matter: an empty invoice raising (seen
by both), a bare number taken as minutes (haiku 2 of 6, Sonnet 0 of 6), and neither model saw a threshold
check left duplicated after a refactor (0/6 each) nor a wrong expiry count (0/3 each).

## Other findings

- **The verifier filtered nothing.** It confirmed 141 of 141 findings, the 24 false alarms
  included, at 37–46 % more cost. A second pass of the same model that rereads the same
  code agrees with the first.
- **Same and other overlap almost fully.** Per repeat, 14–15 bad diffs were detected by both,
  0–2 by haiku only and none by Sonnet only. A second model of the same family adds no
  coverage. It adds fewer false alarms.
- Two haiku reviews joined find 1–2 more bad diffs and double the false alarms.

## Decision (as fixed)

On the 40 diffs, `same`, `other` and their `+verify` arms meet the bar (≥ 50 % detected,
< 0.25 false alarms per good diff); with the judge's labels only Sonnet's do. The cheapest
that meets it under both labelings is **Sonnet alone, without verification** ($0.079 per
detection).

The rule says build «Pedir segunda opinión». The numbers on real failures say it should be
offered for what it is: a reviewer that catches the visible mistakes cheaply (wrong
boundary, a secret in code, a rule broken), and misses most of what an agent gets wrong by
itself. It should be an action the user asks for, with few false alarms (Sonnet), and never
a gate or a guarantee. The verification step should not be built.

Limits: one model family (no other provider here); 6 real bad diffs; planted errors easier
than real ones; Opus as judge is not independent of the reviewers.
