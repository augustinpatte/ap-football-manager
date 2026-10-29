"""Mission 12: before/after summary of apfm_economy runs (finance + market)."""
import json, glob, sys, statistics as st
from collections import Counter

def runs(d):
    return [json.load(open(f)) for f in sorted(glob.glob(f"{d}/s*.json")) if not f.endswith("deals.jsonl")]

def summary(d):
    rs = runs(d)
    print(f"=== {d}: {len(rs)} runs")
    ev = Counter(); cnt = Counter(); secs = []
    for r in rs:
        secs.append(r["headline"]["total_seconds"])
        for e in r["per_season"]:
            for k, v in e["events"].items(): ev[k] += v
        for s, c in r["market_realism"]["counts"].items():
            for k, v in c.items(): cnt[k] += v
    n = len(rs) * 5
    print(f"per season: transfers {ev['transfer_ai']/n:.0f} loans {ev['loan_started']/n:.0f} free {ev['free_agent_signed']/n:.0f}")
    t = cnt["transfer_dom"] + cnt["transfer_intl"]
    print(f"intl share {100*cnt['transfer_intl']/t:.1f}%  (total {t})")
    print("phase (5-season totals): " + ", ".join(f"{p}={cnt['transfer_'+p]}" for p in ["early", "mid", "last_week", "last_72h", "deadline_day"]))
    print("loans by phase: " + ", ".join(f"{p}={cnt['loan_'+p]}" for p in ["early", "mid", "last_week", "last_72h", "deadline_day"]))
    print("seconds", secs)
    for i, label in [(1, "s1"), (3, "s3"), (5, "s5")]:
        row = []
        for r in rs:
            s = r["state"][i]
            j = s["journal_since_last_reading_ai"]
            inc = j.get("Matchday", 0) + j.get("Sponsorship", 0) + j.get("PrizeMoney", 0)
            ex = -(j.get("PlayerWages", 0) + j.get("StaffWages", 0))
            up = -j.get("Upkeep", 0)
            row.append(f"cash med {s['cash']['median']/1e6:.0f}M p90 {s['cash']['p90']/1e6:.0f}M neg {s['cash']['negative_pct']}% | op med {s['operating_result']['median']/1e6:.1f}M deficit {s['operating_result']['deficit_pct']}% | wages/inc {ex/inc:.2f} upkeep/inc {up/inc:.3f} | >110% {s['wage_bill']['clubs_over_110']} >150% {s['wage_bill']['clubs_over_150']}")
        print(label, " || ".join(row))
    print("integrity", [r["integrity_violations"] for r in rs])

for d in sys.argv[1:]:
    summary(d)
