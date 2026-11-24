#!/usr/bin/env python3
"""Prices each confirmation run from its Claude Code session logs, message by message.

A cut session has no price from the CLI, and haiku's price per token is five times higher
once a prompt passes 100 000 tokens, so a run is priced from what its sessions logged.
Rates per million tokens (input, output): haiku 5.5 $0.10/$0.50 (×5 above 100K of prompt),
sonnet 5.5 $2/$10; a cache read is 0.1× input, a cache write 1.25×.

    python3 price.py <projects folder> [more ...] [--layer <projects folder>] > costs.json

Keys are «<startedAt> <case> <arm> <attempt>», as analyze.py reads them; runs logged
under --layer are the layer arm.
"""

import glob, json, re, sys
from collections import defaultdict

PRICE = {"haiku": (0.10, 0.50), "sonnet": (2.0, 10.0), "opus": (4.0, 20.0)}
NAME = re.compile(r"hedy-evals-(\d+)-(.+?)-plain-long-([a-z]+)-(\d+)-\1-")

args, layer = sys.argv[1:], []
while "--layer" in args:
    i = args.index("--layer"); layer.append(args[i + 1]); del args[i:i + 2]

costs = defaultdict(float)
for root in args + layer:
    for folder in glob.glob(root + "/*hedy-evals-*-plain-long-*"):
        m = NAME.search(folder)
        if not m:
            continue
        started, case, arm, attempt = m.groups()
        key = f"{started} {case} {'layer' if root in layer else arm} {attempt}"
        seen = set()
        for path in glob.glob(folder + "/**/*.jsonl", recursive=True):
            for line in open(path, errors="replace"):
                try:
                    r = json.loads(line)
                except ValueError:
                    continue
                msg = r.get("message")
                if r.get("type") != "assistant" or not isinstance(msg, dict) or msg.get("id") in seen:
                    continue
                family = next((f for f in PRICE if f in (msg.get("model") or "")), None)
                if not family:
                    continue
                seen.add(msg.get("id"))
                u = msg.get("usage") or {}
                i, cr, cw, o = (u.get(k) or 0 for k in ("input_tokens", "cache_read_input_tokens",
                                                       "cache_creation_input_tokens", "output_tokens"))
                pin, pout = PRICE[family]
                tier = 5 if family == "haiku" and i + cr + cw > 100_000 else 1
                costs[key] += tier * (pin * (i + 0.1 * cr + 1.25 * cw) + pout * o) / 1e6
json.dump({k: round(v, 4) for k, v in sorted(costs.items())}, sys.stdout, indent=1)
print()
