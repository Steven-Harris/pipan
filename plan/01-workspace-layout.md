# 1. Workspace Layout, Chat-First View & Split Panel System (Items #1, #3, #16)

## Behavior & UI Changes
- **Chat-First Default View**: Opening or starting a session renders a clean, full-height chat interface (`MessageList` + `ChatInput`) without forcing the DAG canvas open.
- **Session Auxiliary Panels**: From the session header toolbar (or command palette/status bar), users can toggle open one or more auxiliary panels alongside the chat:
  - **Node Graph (`FlowCanvas`)**
  - **Terminal (`TerminalPanel`)**
  - **Files (`FilePanel`)**
  - **Pull Requests (`PrPanel`)**
- **Vertical & Horizontal Split Orientation**:
  - Users can select how the auxiliary panel area splits with the Chat window:
    - **Vertical Split (Default)**: Side-by-side layout (Chat on the left, active panel(s) on the right, separated by a draggable vertical splitter).
    - **Horizontal Split**: Top-and-bottom stacked layout (active panel(s) on top, Chat on the bottom, separated by a draggable horizontal splitter).
  - Inside the Node Graph panel itself, Dagre auto-layout also defaults to **Vertical (`TB`)** and can be toggled to **Horizontal (`LR`)**.
  - Split orientation, panel open/closed state, active panel tab, and split ratio persist in user settings.
