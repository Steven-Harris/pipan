import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { chatStore } from '#lib/stores/chat.svelte.ts';
import type {
  PiRpcEvent,
  TerminalExitPayload,
  TerminalOutputPayload,
  TerminalSessionInfo
} from '#lib/types/pi.ts';

export interface AnsiSpan {
  text: string;
  fgClass?: string;
  bgClass?: string;
  bold?: boolean;
  dim?: boolean;
  italic?: boolean;
  underline?: boolean;
}

export interface TerminalLogItem {
  id: string;
  stream: 'stdout' | 'stderr' | 'command' | 'system' | 'pi-bash';
  rawText: string;
  spans: AnsiSpan[];
  timestamp: number;
}

export interface TerminalSessionView {
  sessionId: string;
  info: TerminalSessionInfo | null;
  items: TerminalLogItem[];
  history: string[];
  sendToPiBash: boolean;
  isStarting: boolean;
  isRunningPiBash: boolean;
  error: string | null;
}

const MAX_LOG_ITEMS = 600;
const MAX_HISTORY_ITEMS = 100;

const ANSI_FG_MAP: Record<number, string> = {
  30: 'text-zinc-500',
  31: 'text-rose-400',
  32: 'text-emerald-400',
  33: 'text-amber-400',
  34: 'text-sky-400',
  35: 'text-fuchsia-400',
  36: 'text-cyan-400',
  37: 'text-zinc-200',
  90: 'text-zinc-500',
  91: 'text-rose-300',
  92: 'text-emerald-300',
  93: 'text-amber-300',
  94: 'text-sky-300',
  95: 'text-fuchsia-300',
  96: 'text-cyan-300',
  97: 'text-white'
};

const ANSI_BG_MAP: Record<number, string> = {
  40: 'bg-zinc-900',
  41: 'bg-rose-950/70',
  42: 'bg-emerald-950/70',
  43: 'bg-amber-950/70',
  44: 'bg-sky-950/70',
  45: 'bg-fuchsia-950/70',
  46: 'bg-cyan-950/70',
  47: 'bg-zinc-700/60',
  100: 'bg-zinc-800',
  101: 'bg-rose-900/60',
  102: 'bg-emerald-900/60',
  103: 'bg-amber-900/60',
  104: 'bg-sky-900/60',
  105: 'bg-fuchsia-900/60',
  106: 'bg-cyan-900/60',
  107: 'bg-zinc-600/60'
};

/**
 * Parses ANSI SGR color/style escape sequences into safe UI spans while stripping
 * non-SGR control codes (OSC titles, cursor movement sequences, etc.).
 */
export function parseAnsiSpans(input: string): AnsiSpan[] {
  if (!input) return [];

  // Normalize CRLF -> LF and strip OSC sequences + non-SGR CSI sequences
  const cleaned = input
    .replace(/\r\n/g, '\n')
    .replace(/\r/g, '\n')
    // eslint-disable-next-line no-control-regex
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, '')
    // eslint-disable-next-line no-control-regex
    .replace(/\x1b\[[0-9;?]*[A-LN-Za-ln-z]/g, '');

  // eslint-disable-next-line no-control-regex
  const sgrRegex = /\x1b\[([0-9;]*)m/g;
  const spans: AnsiSpan[] = [];

  let fgClass: string | undefined;
  let bgClass: string | undefined;
  let bold = false;
  let dim = false;
  let italic = false;
  let underline = false;

  let lastIdx = 0;
  let match: RegExpExecArray | null;

  const pushSegment = (text: string) => {
    if (!text) return;
    spans.push({
      text,
      fgClass,
      bgClass,
      bold: bold || undefined,
      dim: dim || undefined,
      italic: italic || undefined,
      underline: underline || undefined
    });
  };

  while ((match = sgrRegex.exec(cleaned)) !== null) {
    if (match.index > lastIdx) {
      pushSegment(cleaned.slice(lastIdx, match.index));
    }
    lastIdx = sgrRegex.lastIndex;

    const rawParams = match[1];
    const codes = rawParams
      ? rawParams.split(';').map((c) => Number.parseInt(c, 10) || 0)
      : [0];

    for (let i = 0; i < codes.length; i++) {
      const code = codes[i];
      if (code === 0) {
        fgClass = undefined;
        bgClass = undefined;
        bold = false;
        dim = false;
        italic = false;
        underline = false;
      } else if (code === 1) {
        bold = true;
      } else if (code === 2) {
        dim = true;
      } else if (code === 3) {
        italic = true;
      } else if (code === 4) {
        underline = true;
      } else if (code === 22) {
        bold = false;
        dim = false;
      } else if (code === 23) {
        italic = false;
      } else if (code === 24) {
        underline = false;
      } else if (code === 39) {
        fgClass = undefined;
      } else if (code === 49) {
        bgClass = undefined;
      } else if (ANSI_FG_MAP[code]) {
        fgClass = ANSI_FG_MAP[code];
      } else if (ANSI_BG_MAP[code]) {
        bgClass = ANSI_BG_MAP[code];
      }
    }
  }

  if (lastIdx < cleaned.length) {
    pushSegment(cleaned.slice(lastIdx));
  }

  return spans;
}

function makeItemId(): string {
  return `term-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

class TerminalStore {
  sessions = $state<Record<string, TerminalSessionView>>({});
  private initialized = false;
  private unlisteners: UnlistenFn[] = [];
  private pendingBashRpcSessionMap = new Map<string, string>();

  async init(): Promise<void> {
    if (this.initialized) return;
    this.initialized = true;

    const unlistenOutput = await listen<TerminalOutputPayload>('terminal:output', (event) => {
      const payload = event.payload;
      if (!payload?.sessionId || !payload.data) return;
      this.appendLog(
        payload.sessionId,
        payload.stream === 'stderr' ? 'stderr' : 'stdout',
        payload.data,
        payload.timestamp
      );
    });

    const unlistenExit = await listen<TerminalExitPayload>('terminal:exit', (event) => {
      const payload = event.payload;
      if (!payload?.sessionId) return;
      const session = this.getOrCreateView(payload.sessionId);
      if (session.info) {
        session.info = {
          ...session.info,
          running: false,
          pid: null
        };
      }
      const codeStr = payload.exitCode !== null ? ` (exit code ${payload.exitCode})` : '';
      this.appendLog(
        payload.sessionId,
        'system',
        `\n[Shell process exited${codeStr}]\n`,
        payload.timestamp
      );
    });

    const unlistenPiEvent = await listen<PiRpcEvent>('pi:event', (event) => {
      const payload = event.payload;
      if (!payload || payload.type !== 'response' || payload.command !== 'bash') return;
      const targetSid =
        (payload.id && this.pendingBashRpcSessionMap.get(payload.id)) ??
        chatStore.activeSessionId ??
        'default';
      if (payload.id) {
        this.pendingBashRpcSessionMap.delete(payload.id);
      }

      const view = this.getOrCreateView(targetSid);
      view.isRunningPiBash = false;

      if (payload.success === false) {
        this.appendLog(
          targetSid,
          'stderr',
          `[Pi bash RPC error] ${payload.error ?? 'Command failed'}\n`
        );
        return;
      }

      const data = payload.data ?? {};
      const output = typeof data.output === 'string' ? data.output : '';
      const exitCode = typeof data.exitCode === 'number' ? data.exitCode : 0;
      const cancelled = Boolean(data.cancelled);

      if (output) {
        this.appendLog(
          targetSid,
          exitCode === 0 ? 'pi-bash' : 'stderr',
          output.endsWith('\n') ? output : `${output}\n`
        );
      }
      if (cancelled) {
        this.appendLog(targetSid, 'system', '[Pi bash RPC cancelled — output added to context]\n');
      } else {
        this.appendLog(
          targetSid,
          'system',
          `[Pi bash RPC completed (exit ${exitCode}) — added to conversation context]\n`
        );
      }
    });

    this.unlisteners.push(unlistenOutput, unlistenExit, unlistenPiEvent);
  }

  destroy(): void {
    for (const unlisten of this.unlisteners) {
      unlisten();
    }
    this.unlisteners = [];
    this.initialized = false;
  }

  getOrCreateView(sessionId: string): TerminalSessionView {
    const sid = sessionId.trim() || 'default';
    if (!this.sessions[sid]) {
      this.sessions[sid] = {
        sessionId: sid,
        info: null,
        items: [],
        history: [],
        sendToPiBash: false,
        isStarting: false,
        isRunningPiBash: false,
        error: null
      };
    }
    return this.sessions[sid];
  }

  appendLog(
    sessionId: string,
    stream: TerminalLogItem['stream'],
    rawText: string,
    timestamp = Date.now()
  ): void {
    const view = this.getOrCreateView(sessionId);
    const spans = parseAnsiSpans(rawText);
    if (spans.length === 0) return;

    const item: TerminalLogItem = {
      id: makeItemId(),
      stream,
      rawText,
      spans,
      timestamp
    };

    const next = [...view.items, item];
    view.items = next.length > MAX_LOG_ITEMS ? next.slice(next.length - MAX_LOG_ITEMS) : next;
  }

  async ensureSession(
    sessionId: string,
    cwd: string | null,
    cols = 100,
    rows = 28
  ): Promise<void> {
    await this.init();
    const view = this.getOrCreateView(sessionId);
    if (view.isStarting) return;

    // If already running and cwd matches (or both headless), no need to respawn
    if (view.info?.running && (!cwd || view.info.cwd === cwd)) {
      return;
    }

    await this.startSession(sessionId, cwd, undefined, cols, rows);
  }

  async startSession(
    sessionId: string,
    cwd: string | null,
    shell?: string,
    cols = 100,
    rows = 28
  ): Promise<void> {
    await this.init();
    const view = this.getOrCreateView(sessionId);
    view.isStarting = true;
    view.error = null;

    try {
      const info = await invoke<TerminalSessionInfo>('start_terminal_session', {
        sessionId: view.sessionId,
        cwd: cwd ?? null,
        shell: shell ?? null,
        cols,
        rows
      });
      view.info = info;
    } catch (err) {
      view.error = err instanceof Error ? err.message : String(err);
    } finally {
      view.isStarting = false;
    }
  }

  async executeCommand(
    sessionId: string,
    rawCommand: string,
    cwd: string | null
  ): Promise<void> {
    const command = rawCommand.trim();
    if (!command) return;

    const view = this.getOrCreateView(sessionId);
    view.error = null;

    // Push to command history (deduplicating consecutive identical commands)
    if (view.history[view.history.length - 1] !== command) {
      const nextHistory = [...view.history, command];
      view.history =
        nextHistory.length > MAX_HISTORY_ITEMS
          ? nextHistory.slice(nextHistory.length - MAX_HISTORY_ITEMS)
          : nextHistory;
    }

    if (view.sendToPiBash) {
      this.appendLog(view.sessionId, 'command', `$ ${command}  [→ Pi bash RPC]\n`);
      if (chatStore.sessionStatus.state !== 'running') {
        view.error =
          'Pi session is not running. Start a session or turn off "Send to Pi (bash RPC)" to run in the local shell.';
        return;
      }
      view.isRunningPiBash = true;
      try {
        const rpcId = await chatStore.runBashCommand(command);
        this.pendingBashRpcSessionMap.set(rpcId, view.sessionId);
      } catch (err) {
        view.isRunningPiBash = false;
        view.error = err instanceof Error ? err.message : String(err);
      }
      return;
    }

    // Ensure local shell is running
    if (!view.info?.running) {
      await this.startSession(view.sessionId, cwd);
    }

    this.appendLog(view.sessionId, 'command', `$ ${command}\n`);
    try {
      await invoke('send_terminal_input', {
        sessionId: view.sessionId,
        data: `${command}\n`
      });
    } catch (err) {
      view.error = err instanceof Error ? err.message : String(err);
    }
  }

  async sendInterrupt(sessionId: string): Promise<void> {
    const view = this.getOrCreateView(sessionId);
    view.error = null;

    if (view.isRunningPiBash && chatStore.sessionStatus.state === 'running') {
      try {
        await chatStore.sendCommand({ type: 'abort_bash' });
      } catch {
        // Ignore if not supported
      }
      view.isRunningPiBash = false;
      this.appendLog(view.sessionId, 'system', '^C [Interrupted Pi bash RPC]\n');
      return;
    }

    if (!view.info?.running) return;
    this.appendLog(view.sessionId, 'system', '^C\n');
    try {
      await invoke('send_terminal_input', {
        sessionId: view.sessionId,
        data: '\u0003'
      });
    } catch (err) {
      view.error = err instanceof Error ? err.message : String(err);
    }
  }

  async resize(sessionId: string, cols: number, rows: number): Promise<void> {
    const view = this.getOrCreateView(sessionId);
    if (!view.info?.running) return;
    try {
      const updated = await invoke<TerminalSessionInfo>('resize_terminal', {
        sessionId: view.sessionId,
        cols: Math.round(cols),
        rows: Math.round(rows)
      });
      view.info = updated;
    } catch {
      // Non-fatal resize failure
    }
  }

  async stopSession(sessionId: string): Promise<void> {
    const view = this.getOrCreateView(sessionId);
    try {
      await invoke<boolean>('stop_terminal_session', {
        sessionId: view.sessionId
      });
      if (view.info) {
        view.info = {
          ...view.info,
          running: false,
          pid: null
        };
      }
    } catch (err) {
      view.error = err instanceof Error ? err.message : String(err);
    }
  }

  clearOutput(sessionId: string): void {
    const view = this.getOrCreateView(sessionId);
    view.items = [];
    view.error = null;
  }

  toggleSendToPiBash(sessionId: string): void {
    const view = this.getOrCreateView(sessionId);
    view.sendToPiBash = !view.sendToPiBash;
  }
}

export const terminalStore = new TerminalStore();
