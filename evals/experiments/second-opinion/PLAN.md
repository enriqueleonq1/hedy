# Test B: does a second opinion find real errors without false alarms?

Written before any run of this test (Nov 23, 2026). Nothing here changes after the data.

**Question.** Shown a finished change, can a reviewer with a clean context find the changes
that are wrong, without flagging the good ones?

Only Claude models are available here, so «another provider» is replaced by another model
of the same family. Its blind spots are more likely shared with the author's than another
provider's would be; the result says nothing about other providers.

## Material

About 40 diffs saved by test A (`--diffs`), half bad (they fail the hidden check, not by an
error or timeout) and half good (they pass it), at most 3 per case and class, as many cases
as possible. If bad ones are short, errors are planted by hand in correct solutions (an
off-by-one, a missed edge case, a wrong rounding, a dropped validation, a secret written in
code…), each checked to pass the project's tests and fail the hidden check. Which diffs go
in is decided from the verdicts only, before any review runs.

## Arms

Each reviewer runs with a clean context (`claude -p`, no settings, no memory, no MCP), in a
copy of the repo with the change applied, with read-only tools (`Read`, `Grep`, `Glob`). It
gets the request and the diff and answers with the same JSON:
`{"findings": [{"file", "line", "claim", "severity": "high|medium|low", "confidence": 0–1}]}`.

- **same**: haiku, the model that wrote the changes.
- **other**: Sonnet.
- **same+verify**, **other+verify**: each high or medium finding goes to a verifier of the
  same model, in a new session on its own copy, with `Bash` as well (it may run code to
  reproduce), which answers `{"confirmed": true|false, "reason"}`; only confirmed findings
  stay.
- At the same number of calls (2), from the same runs: two `same` reviews joined, `same` +
  `other` joined, and each `+verify` arm.

3 repeats per diff and arm.

## Measures

- A diff is **flagged** when it has at least one high or medium finding.
- **Detected**: a bad diff with at least one finding that names the real cause (labelled by
  hand against the hidden check's output and the diff).
- **False alarm**: a high or medium finding on a good diff that is not a real defect.
  Findings on good diffs that are real but not covered by the hidden check are counted apart.
- Precision (real findings over all high or medium findings), overlap between arms (bad
  diffs detected by both, by one only), cost per detected error.
- Labels: by hand, for every finding. A blind judge (Opus; it does not know the arm; it gets
  the request, the diff, the hidden check's output and the finding) labels them too; the
  agreement (Cohen's kappa) is reported. It is not a third provider: same family again.
- Uncertainty: 95 % intervals by bootstrap over cases. With about 20 diffs per class only
  differences of about 20 points or more between arms can be told apart.

## Decision (fixed now)

Build «Pedir segunda opinión» if an arm detects at least 50 % of the bad diffs with fewer
than 1 false alarm per 4 good diffs (false alarms / good diffs < 0.25), averaged over the
repeats. Among arms that pass, the cheapest per detected error.
