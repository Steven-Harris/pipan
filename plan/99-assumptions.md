# Explicit Assumptions & Defaults Chosen
- **Split Default**: Opening the Node View (or other session panels) defaults to a **vertical side-by-side split** (Chat left, Panel right), with a one-click toggle for horizontal top/bottom split.
- **Session Storage**: Sessions use real Pi `.jsonl` session files paired with `<app_local_data_dir>/pipan-metadata.json` so transcripts, costs, and node coordinates survive restarts.
- **Built-in Extensions Enabled by Default**: All 5 repackaged built-in extensions (`pipan.node-graph`, `pipan.github-prs`, `pipan.terminal`, `pipan.files`, `pipan.cost-analytics`) ship enabled by default so all features work out of the box, while users can disable/uninstall them or install custom plugins from the Extensions manager.
