# Memecoin screening – data checklist

Source: RugCheck full report (1 API call) unless noted.

---

## Phase 1 – RugCheck

| # | Data | Have? | Where / notes |
|---|------|-------|----------------|
| 1 | Risk score (e.g. 1–100) | [x] | full report: score_normalised, score |
| 2 | Red flags (list) | [x] | full report: risks[] (name, level, description, value) |
| 3 | Top holder % | [x] | full report: topHolders[].pct (max = top holder) |
| 4 | Creator address | [x] | full report: creator |
| 5 | Creator balance | [x] | full report: creatorBalance |
| 6 | Anomalies | [x] | full report: risks[] (e.g. "Low amount of LP Providers") |
| 7 | LP locked % | [x] | full report: markets[0].lp.lpLockedPct |
| 8 | Total LP providers | [x] | full report: totalLPProviders |
| 9 | Insider signal | [x] | full report: graphInsidersDetected, topHolders[].insider |
| 10 | Creator rug history (has this dev rugged before?) | [ ] | not in RugCheck API – need other source or skip |

---

## Phase 2 – Holder & wallet breakdown (GMGN-style)

| # | Data | Have? | Where / notes |
|---|------|-------|----------------|
| 11 | Number of holders | [x] | full report: totalHolders |
| 12 | Market cap | [x] | derive: price × (supply / 10^decimals) from full report |
| 13 | MC per holder | [x] | derive: market_cap / totalHolders |
| 14 | Insiders % of holders | [ ] | GMGN (or alternative) |
| 15 | Phishing % of holders | [ ] | GMGN (or alternative) |
| 16 | Bundler % of holders | [ ] | GMGN (or alternative) |
| 17 | Bluechip % of holders | [ ] | GMGN (or alternative) |
| 18 | Fresh wallet count | [ ] | GMGN (or alternative) |
| 19 | Bundled wallet count | [ ] | GMGN (or alternative) |
| 20 | Fresh / holders ratio | [ ] | need 18 + totalHolders (we have holders) |
| 21 | Bundled / holders ratio | [ ] | need 19 + totalHolders (we have holders) |
| 22 | Top holders with labels (phishing, axiom, whale, bundled, bluechip) | [ ] | GMGN; RugCheck has topHolders + insider only |

---

## Summary

- Have from RugCheck: 1–9, 11–13 (risk, red flags, top holder %, creator, LP, holders, MC/holder, insider).
- Missing: 10 (creator rug history), 14–22 (GMGN-style breakdown: insiders/phishing/bundler/bluechip %, fresh/bundled counts and ratios, top-holder labels).
- Next: find free API (or alternative) for GMGN-style holder/wallet data by token mint.
