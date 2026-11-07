# Pipan v0.2 Implementation Plan — Modular Workspace, Persistent Sessions, Deep GitHub Integration & Extension System

Split by phase so each can be read independently. Read only the phase file(s) relevant to the task.

## Summary

This plan replaces the initial Phase 1–5 prototype plan with the complete architecture for the next version of **Pipan**. It transitions the app into a modular, VS Code-style extensible workspace that defaults to a clean Chat-first session view with on-demand split panels (Nodes DAG, Terminal, File Explorer, Pull Requests), reliable node layout with per-session coordinate persistence, persistent Pi sessions with full lifecycle management (search, group, filter, sort, rename, archive, delete, stop), deep `gh` CLI PR review/merge workflows, skill & model selectors, drag-and-drop prompt attachments, a historical Cost & Usage analytics page, an interactive dependency Doctor with auto-remediation, in-app Pi/GitHub login & configuration, and a bottom status bar.

## Phases

- [1. Workspace Layout, Chat-First View & Split Panel System (Items #1, #3, #16)](01-workspace-layout.md)
- [2. Node Graph Rendering, Viewport Fitting & Per-Session Persistence (Item #2)](02-node-graph.md)
- [3. Persistent Sessions: Stop, Search, Group, Filter, Sort, Rename, Archive & Delete (Items #6, #7, #8, #9)](03-persistent-sessions.md)
- [4. Model Selector, Skills Picker & Drag-and-Drop File Attachments (Items #5, #10, #11)](04-model-skills-attachments.md)
- [5. Deep GitHub Integration & PR Panel (Item #4)](05-github-integration.md)
- [6. Session Terminal & File Explorer Panels (Item #16)](06-terminal-file-explorer.md)
- [7. Cost & Usage Analytics Page & Bottom Status Bar (Items #12, #13)](07-cost-status-bar.md)
- [8. In-App Login/Configuration & Dependency Doctor (Items #14, #15)](08-login-doctor.md)
- [9. VS Code-Style Extension / Plugin System (Item #17)](09-extension-system.md)
- [10. Verification & Test Scenarios](10-verification.md)
- [Explicit Assumptions & Defaults Chosen](99-assumptions.md)
