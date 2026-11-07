# 7. Cost & Usage Analytics Page & Bottom Status Bar (Items #12, #13)

## Usage Indexer & Cost Aggregation (`src-tauri/src/usage.rs`, `src/lib/stores/usage.svelte.ts`)
- **Backend Usage Aggregator (`usage.rs`)**:
  - Command `get_usage_analytics()`: Scans both `~/.pi/agent/sessions/**/*.jsonl` (parsing `message` entries with `usage.cost.total`, `usage.input`, `usage.output`, `usage.cacheRead`, `usage.cacheWrite`, `timestamp`, `model`, `provider`) AND Pipan's local SQLite `usage_ledger` table in `pipan.db` (so even headless sessions are recorded). Parsed Pi `.jsonl` entries are upserted into `usage_ledger` (`session_id`, `message_id`, `timestamp`, `provider`, `model`, `input`, `output`, `cache_read`, `cache_write`, `cost_usd`; PK `(session_id, message_id)`) and a per-file `(path, mtime, byte_offset)` checkpoint lets rescans parse only new lines.
  - Deduplication falls out of the `(session_id, message_id)` primary key (`INSERT ... ON CONFLICT DO NOTHING`). Time-series buckets and totals are computed with SQL `GROUP BY` over `timestamp` (indexed) for:
    - **Day** (hourly breakdown for today + last 30 days daily)
    - **Week** (daily breakdown for current week + last 12 weeks)
    - **Month** (daily/weekly breakdown for current month + last 12 months)
    - **Year** (monthly breakdown by year)
  - Also aggregates live active-session stats via Pi RPC `get_session_stats`.

## Cost Page UI (`src/lib/components/views/CostView.svelte`)
- Accessible from the left navigation rail or by clicking the cost indicator in the bottom status bar.
- Period selector tabs: **Day | Week | Month | Year**.
- Summary KPI cards: Total Spend ($), Total Tokens (Input / Output / Cache Read / Cache Write), Active & Total Sessions, Average Cost per Session.
- Interactive bar chart of spend over the selected time granularity + breakdown tables by **Model / Provider** and **Workspace / Session**.

## Bottom Status Bar (`src/lib/components/StatusBar.svelte`) (Item #13)
- Persistent 28px bottom bar across the entire app showing:
  - **Left**: Pi connection status pill, active workspace/worktree branch, active background subagent count, GitHub account & webhook status.
  - **Right**: Active session token/context window % (`contextUsage.percent`), current session cost & total cumulative cost (clicks to open **Cost Page**), total session count (`active / total`), active Model & Thinking level (clicks to open **Model Selector**), Extensions count (clicks to open **Extensions**), and **Doctor** health status indicator.
