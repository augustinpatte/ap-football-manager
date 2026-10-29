"""Mission 12: breakdown of AI transfers from the harness's *.deals.jsonl (price/value, border)."""
import json, sys, glob, statistics as st
from collections import defaultdict

def load(d):
    rows = []
    for f in sorted(glob.glob(f"{d}/*.deals.jsonl")):
        rows += [json.loads(l) for l in open(f) if l.strip()]
    return rows

def med(x): return round(st.median(x), 2) if x else None
def band_rep(r): return "S<550" if r < 550 else ("M550-699" if r < 700 else "L>=700")
def band_val(v): return "v<0.5M" if v < 5e5 else ("v0.5-3M" if v < 3e6 else ("v3-15M" if v < 15e6 else "v>=15M"))
def band_c(d):
    if d is None: return "none"
    return "<=60d" if d <= 60 else ("<=180d" if d <= 180 else ("<=365d" if d <= 365 else (">1y-3y" if d < 1095 else ">=3y")))

def table(rows, key, title):
    g = defaultdict(list)
    for r in rows: g[key(r)].append(r)
    print(f"\n## {title}")
    print(f"{'cat':<18}{'n':>6}{'fee/val':>9}{'ask/val':>9}{'intl%':>7}")
    for k in sorted(g):
        v = g[k]
        ratios = [r["fee"] / r["value"] for r in v if r["value"] > 0]
        asks = [r["asking"] / r["value"] for r in v if r["value"] > 0]
        intl = 100 * sum(r["buyer_country"] != r["seller_country"] for r in v) / len(v)
        print(f"{str(k):<18}{len(v):>6}{med(ratios):>9}{med(asks):>9}{intl:>7.1f}")

if __name__ == "__main__":
    rows = load(sys.argv[1])
    print("deals", len(rows))
    table(rows, lambda r: "all", "all")
    table(rows, lambda r: r["status"], "by status")
    table(rows, lambda r: band_c(r["contract_days"]), "by contract left")
    table(rows, lambda r: r["phase"], "by phase")
    table(rows, lambda r: band_rep(r["buyer_rep"]), "by buyer reputation")
    table(rows, lambda r: band_val(r["value"]), "by value")
    table(rows, lambda r: "thin" if r["thin"] else "not thin", "by buyer need")
    table(rows, lambda r: "listed" if r["listed"] else "unlisted", "listed")
    table(rows, lambda r: r["buyer_country"], "by buyer country")
    table(rows, lambda r: r["seller_country"], "by seller country")
    table(rows, lambda r: "age<=21" if r["age"] <= 21 else ("22-26" if r["age"] <= 26 else ("27-30" if r["age"] <= 30 else "31+")), "by age")
    table(rows, lambda r: (band_rep(r["buyer_rep"]), band_val(r["value"])), "buyer rep x value")
    # fee ceiling vs asking: how often is the fee below asking (haggled)
    below = sum(r["fee"] < r["asking"] for r in rows)
    print("\nfee<asking:", below, "of", len(rows))
