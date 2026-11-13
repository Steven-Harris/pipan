/**
 * Extension channel store.
 *
 * Decodes the channel traffic that the PiPan shim (src-tauri/resources/pipan-channel-shim.ts)
 * puts on Pi's RPC stream and keeps it per session: the latest state per (channel, topic), the
 * queue of pending requests, and the durable log. Nothing is processed until the shim has
 * announced itself for the session.
 *
 * The store never talks to Tauri directly. `configure` hands it a transport that writes JSON
 * commands to Pi, and the chat store forwards every `pi:event` to `handleEvent`.
 */

import {
  EXPIRY_KEY_PREFIX,
  REQUEST_TITLE_PREFIX,
  SHIM_WIDGET_KEY,
  STATE_KEY_PREFIX,
  SeqTracker,
  decodeExpiry,
  decodeRequest,
  decodeShimAnnounce,
  decodeState,
  encodeIntent,
  encodeRequestAnswer,
  encodeResync,
  parseLogType,
  parseStateKey,
  INTENT_COMMAND
} from '#lib/channels/codec.ts';
import {
  MAX_LOG_ITEMS,
  MAX_PENDING_REQUESTS,
  MAX_PRODUCER_ERRORS,
  MAX_REQUESTS_PER_WINDOW,
  REQUEST_RATE_WINDOW_MS
} from '#lib/channels/limits.ts';
import type { HostInfo } from '#lib/channels/types.ts';
import { validateRequest, validateState } from '#lib/channels/validate.ts';

export interface ChannelTransport {
  /** Writes one JSON command line to Pi's stdin. */
  send(command: Record<string, unknown>): Promise<void>;
}

/** The subset of a Pi RPC event this store reads. */
export interface ChannelPiEvent {
  type: string;
  [key: string]: unknown;
}

export interface ChannelState {
  sessionId: string;
  ch: string;
  topic: string;
  primitive: string;
  seq: number | undefined;
  /** Validated and normalized for known primitives, raw otherwise. */
  data: unknown;
  notices: string[];
  updatedAt: number;
}

export interface PendingRequest {
  sessionId: string;
  /** Id of the Pi `extension_ui_request`, needed to answer it. */
  piRequestId: string;
  requestId: string;
  ch: string;
  kind: string;
  payload: unknown;
  notices: string[];
  receivedAt: number;
}

export interface LogItem {
  entryId: string;
  kind: 'entry' | 'message';
  ch: string;
  name: string;
  data: unknown;
  /** Message text for `kind: "message"`. */
  content?: unknown;
  timestamp?: string;
}

export interface ProducerError {
  id: number;
  sessionId: string;
  ch: string;
  message: string;
  at: number;
}

/** Prefix of the ids this store puts on its own Pi commands, so responses can be recognised. */
const ID_PREFIX = 'ch-';
const LOG_SYNC_DELAY_MS = 50;

const stateKey = (sessionId: string, ch: string, topic: string) => `${sessionId}\u0000${ch}\u0000${topic}`;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

class ChannelsStore {
  /** Sessions whose shim has announced, with what the host advertised. */
  hosts = $state.raw<Record<string, HostInfo>>({});
  /** Latest state per `sessionId, ch, topic`. */
  states = $state.raw<Record<string, ChannelState>>({});
  /** Pending requests per session, oldest first. */
  pending = $state.raw<Record<string, PendingRequest[]>>({});
  /** Durable log entries per session, oldest first. */
  log = $state.raw<Record<string, LogItem[]>>({});
  /** Recent producer errors, newest last. */
  errors = $state.raw<ProducerError[]>([]);

  private transport: ChannelTransport | null = null;
  private seqs = new Map<string, SeqTracker>();
  private requestTimes = new Map<string, number[]>();
  private cursors = new Map<string, string>();
  private commandChecks = new Set<string>();
  private logRequests = new Map<string, { sessionId: string; since: string | undefined }>();
  private logBusy = new Set<string>();
  private logDirty = new Set<string>();
  private logTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private nextId = 0;
  private nextErrorId = 0;

  configure(transport: ChannelTransport | null): void {
    this.transport = transport;
  }

  /** True once the shim has announced for the session. */
  isActive(sessionId: string): boolean {
    return sessionId in this.hosts;
  }

  statesFor(sessionId: string): ChannelState[] {
    return Object.values(this.states).filter((s) => s.sessionId === sessionId);
  }

  // ---- Inbound -----------------------------------------------------------------------------

  /**
   * Routes one Pi event. Returns true when the event was channel traffic that the caller should
   * not process further.
   */
  handleEvent(sessionId: string, event: ChannelPiEvent): boolean {
    switch (event.type) {
      case 'extension_ui_request':
        return this.handleUiRequest(sessionId, event);
      case 'response':
        return this.handleResponse(event);
      case 'entry_appended': {
        const entry = event.entry;
        if (isRecord(entry) && typeof entry.customType === 'string' && parseLogType(entry.customType)) {
          this.scheduleLogSync(sessionId);
        }
        return false;
      }
      case 'message_end': {
        const message = event.message;
        if (isRecord(message) && typeof message.customType === 'string' && parseLogType(message.customType)) {
          this.scheduleLogSync(sessionId);
        }
        return false;
      }
      default:
        return false;
    }
  }

  private handleUiRequest(sessionId: string, event: ChannelPiEvent): boolean {
    const method = event.method;
    if (method === 'setWidget' && typeof event.widgetKey === 'string') {
      const key = event.widgetKey;
      const lines = Array.isArray(event.widgetLines) ? (event.widgetLines as string[]) : undefined;
      if (key === SHIM_WIDGET_KEY) {
        this.onAnnounce(sessionId, lines);
        return true;
      }
      if (key.startsWith(STATE_KEY_PREFIX)) {
        this.onState(sessionId, key, lines);
        return true;
      }
      if (key.startsWith(EXPIRY_KEY_PREFIX)) {
        this.onExpiry(sessionId, key, lines);
        return true;
      }
      return false;
    }
    if (method === 'editor' && typeof event.title === 'string' && event.title.startsWith(REQUEST_TITLE_PREFIX)) {
      this.onRequest(sessionId, event);
      return true;
    }
    return false;
  }

  private onAnnounce(sessionId: string, lines: string[] | undefined): void {
    const info = decodeShimAnnounce(lines);
    if (!info.ok) {
      this.addError(sessionId, 'shim', `Invalid shim announce: ${info.error}`);
      return;
    }
    this.hosts = { ...this.hosts, [sessionId]: info.value };
    // The shim announces on every session start and every resync, and re-emits its cache right
    // after. Its cache is the source of truth, so start the session's state from scratch.
    this.clearStates(sessionId);
    this.seqs.delete(sessionId);
    this.scheduleLogSync(sessionId);
  }

  private onState(sessionId: string, key: string, lines: string[] | undefined): void {
    if (!this.isActive(sessionId)) return;
    const parsedKey = parseStateKey(key);
    if (!parsedKey) {
      this.addError(sessionId, 'unknown', `Invalid state key ${key}`);
      return;
    }
    const { ch, topic } = parsedKey;
    if (!lines || lines.length === 0) {
      const { [stateKey(sessionId, ch, topic)]: _removed, ...rest } = this.states;
      this.states = rest;
      return;
    }

    const decoded = decodeState(key, lines);
    if (!decoded.ok) {
      this.addError(sessionId, ch, `Extension ${ch} sent an invalid state for ${topic}: ${decoded.error}`);
      return;
    }
    let tracker = this.seqs.get(sessionId);
    if (!tracker) this.seqs.set(sessionId, (tracker = new SeqTracker()));
    if (!tracker.accept(ch, topic, decoded.value.seq)) return;

    const valid = validateState(decoded.value.primitive, decoded.value.data);
    if (!valid.ok) {
      this.addError(sessionId, ch, `Extension ${ch} sent an invalid ${decoded.value.primitive}: ${valid.error}`);
      return;
    }
    this.states = {
      ...this.states,
      [stateKey(sessionId, ch, topic)]: {
        sessionId,
        ch,
        topic,
        primitive: decoded.value.primitive,
        seq: decoded.value.seq,
        data: valid.value,
        notices: valid.notices,
        updatedAt: Date.now()
      }
    };
  }

  private onRequest(sessionId: string, event: ChannelPiEvent): void {
    const piRequestId = typeof event.id === 'string' ? event.id : '';
    const title = event.title as string;
    const decoded = decodeRequest(title, typeof event.prefill === 'string' ? event.prefill : undefined);
    if (!decoded.ok) {
      this.addError(sessionId, 'unknown', `Invalid request ${title}: ${decoded.error}`);
      this.cancelPiRequest(piRequestId);
      return;
    }
    const { ch, kind, requestId, payload } = decoded.value;

    if (!this.isActive(sessionId)) {
      this.cancelPiRequest(piRequestId);
      return;
    }
    if (!this.allowRequest(sessionId)) {
      this.addError(sessionId, ch, `Extension ${ch} sent too many requests; this one was cancelled`);
      this.cancelPiRequest(piRequestId);
      return;
    }
    const queue = this.pending[sessionId] ?? [];
    if (queue.length >= MAX_PENDING_REQUESTS) {
      this.addError(sessionId, ch, `Extension ${ch} has too many pending requests; this one was cancelled`);
      this.cancelPiRequest(piRequestId);
      return;
    }
    const valid = validateRequest(kind, payload);
    if (!valid.ok) {
      this.addError(sessionId, ch, `Extension ${ch} sent an invalid ${kind}: ${valid.error}`);
      this.cancelPiRequest(piRequestId);
      return;
    }

    this.pending = {
      ...this.pending,
      [sessionId]: [
        ...queue,
        {
          sessionId,
          piRequestId,
          requestId,
          ch,
          kind,
          payload: valid.value,
          notices: valid.notices,
          receivedAt: Date.now()
        }
      ]
    };
  }

  private onExpiry(sessionId: string, key: string, lines: string[] | undefined): void {
    if (!lines) return; // The shim clears the widget right after setting it.
    const expiry = decodeExpiry(key, lines);
    if (!expiry.ok) return;
    const queue = this.pending[sessionId] ?? [];
    const expired = queue.find((r) => r.requestId === expiry.value.requestId);
    if (!expired) return;
    this.removePending(sessionId, expired.requestId);
    // The shim already gave up, but Pi still tracks the dialog until it gets a response.
    this.cancelPiRequest(expired.piRequestId);
  }

  private handleResponse(event: ChannelPiEvent): boolean {
    const id = typeof event.id === 'string' ? event.id : '';
    if (!id.startsWith(ID_PREFIX)) return false;

    if (this.commandChecks.delete(id)) {
      const data = event.data;
      const commands = isRecord(data) && Array.isArray(data.commands) ? data.commands : [];
      const hasIntent = commands.some(
        (c) => isRecord(c) && typeof c.name === 'string' && c.name.replace(/^\//, '') === INTENT_COMMAND
      );
      if (event.success === true && hasIntent) this.sendPrompt(encodeResync());
      return true;
    }

    const logRequest = this.logRequests.get(id);
    if (logRequest) {
      this.logRequests.delete(id);
      this.onEntries(logRequest, event);
    }
    return true;
  }

  // ---- Requests ----------------------------------------------------------------------------

  /** Answers a pending request with the primitive's answer payload. */
  async answer(sessionId: string, requestId: string, answer: unknown): Promise<void> {
    const request = this.removePending(sessionId, requestId);
    if (!request) return;
    await this.send({
      type: 'extension_ui_response',
      id: request.piRequestId,
      value: encodeRequestAnswer(request.ch, request.requestId, answer)
    });
  }

  async cancel(sessionId: string, requestId: string): Promise<void> {
    const request = this.removePending(sessionId, requestId);
    if (request) await this.send(cancelledResponse(request.piRequestId));
  }

  /**
   * Cancels every pending request. Pi keeps a dialog open across a session switch, so the host
   * must answer each one. Pi runs one session at a time, so every pending request belongs to it.
   */
  async cancelAll(): Promise<void> {
    const all = Object.values(this.pending).flat();
    this.pending = {};
    await Promise.all(all.map((r) => this.send(cancelledResponse(r.piRequestId))));
  }

  /**
   * Call before sending `switch_session` or `new_session`. Pi replaces the extension instances,
   * so pending requests are cancelled and all state is dropped until the new shim announces.
   */
  async beforeSessionChange(): Promise<void> {
    await this.cancelAll();
    this.hosts = {};
    this.states = {};
    this.seqs.clear();
  }

  // ---- Outbound ----------------------------------------------------------------------------

  /** Sends an intent to a producer. Returns false when the shim has not announced. */
  async sendIntent(sessionId: string, ch: string, name: string, payload: unknown): Promise<boolean> {
    if (!this.isActive(sessionId)) return false;
    await this.send({ type: 'prompt', id: this.newId('intent'), message: encodeIntent(ch, name, payload) });
    return true;
  }

  /**
   * Call when the host attaches to a session that is already running (app reload) or has just
   * switched to one. Asks Pi whether the shim is loaded and, if so, requests a resync, which
   * makes the shim announce itself and re-emit its cached state.
   */
  async attach(): Promise<void> {
    const id = this.newId('cmds');
    this.commandChecks.add(id);
    await this.send({ type: 'get_commands', id });
  }

  // ---- Lifecycle ---------------------------------------------------------------------------

  /**
   * Call when the Pi process is stopping, has exited, or is starting a new one. Pending requests
   * died with it, so nothing is sent. The log and its cursor are kept: both are reloaded from the
   * session file.
   */
  processExited(): void {
    this.hosts = {};
    this.states = {};
    this.pending = {};
    this.seqs.clear();
    this.requestTimes.clear();
    this.commandChecks.clear();
    this.logRequests.clear();
    this.logBusy.clear();
    this.logDirty.clear();
    for (const timer of this.logTimers.values()) clearTimeout(timer);
    this.logTimers.clear();
  }

  /** Forgets everything stored for a deleted session. */
  forgetSession(sessionId: string): void {
    this.clearStates(sessionId);
    const { [sessionId]: _host, ...hosts } = this.hosts;
    this.hosts = hosts;
    const { [sessionId]: _pending, ...pending } = this.pending;
    this.pending = pending;
    const { [sessionId]: _log, ...log } = this.log;
    this.log = log;
    this.seqs.delete(sessionId);
    this.cursors.delete(sessionId);
  }

  dismissErrors(): void {
    this.errors = [];
  }

  // ---- Log ---------------------------------------------------------------------------------

  private scheduleLogSync(sessionId: string): void {
    if (this.logTimers.has(sessionId)) return;
    this.logTimers.set(
      sessionId,
      setTimeout(() => {
        this.logTimers.delete(sessionId);
        void this.fetchLog(sessionId);
      }, LOG_SYNC_DELAY_MS)
    );
  }

  private async fetchLog(sessionId: string): Promise<void> {
    if (this.logBusy.has(sessionId)) {
      this.logDirty.add(sessionId);
      return;
    }
    this.logBusy.add(sessionId);
    const since = this.cursors.get(sessionId);
    const id = this.newId('entries');
    this.logRequests.set(id, { sessionId, since });
    const command: Record<string, unknown> = { type: 'get_entries', id };
    if (since !== undefined) command.since = since;
    await this.send(command);
  }

  private onEntries(request: { sessionId: string; since: string | undefined }, event: ChannelPiEvent): void {
    const { sessionId, since } = request;
    this.logBusy.delete(sessionId);

    if (event.success !== true) {
      if (since !== undefined) {
        // The cursor is unknown to Pi (a different or rewritten session file): reload everything.
        this.cursors.delete(sessionId);
        this.logDirty.add(sessionId);
      }
    } else {
      const data = event.data;
      const entries = isRecord(data) && Array.isArray(data.entries) ? data.entries : [];
      const items: LogItem[] = [];
      for (const entry of entries) {
        if (!isRecord(entry) || typeof entry.id !== 'string') continue;
        this.cursors.set(sessionId, entry.id);
        if (typeof entry.customType !== 'string') continue;
        const type = parseLogType(entry.customType);
        if (!type) continue;
        if (entry.type === 'custom') {
          items.push({ entryId: entry.id, kind: 'entry', ...type, data: entry.data, timestamp: asString(entry.timestamp) });
        } else if (entry.type === 'custom_message') {
          items.push({
            entryId: entry.id,
            kind: 'message',
            ...type,
            data: entry.details,
            content: entry.content,
            timestamp: asString(entry.timestamp)
          });
        }
      }
      const base = since === undefined ? [] : (this.log[sessionId] ?? []);
      this.log = { ...this.log, [sessionId]: [...base, ...items].slice(-MAX_LOG_ITEMS) };
    }

    if (this.logDirty.delete(sessionId)) void this.fetchLog(sessionId);
  }

  // ---- Internals ---------------------------------------------------------------------------

  private newId(kind: string): string {
    return `${ID_PREFIX}${kind}-${++this.nextId}`;
  }

  private send(command: Record<string, unknown>): Promise<void> {
    if (!this.transport) return Promise.resolve();
    return this.transport.send(command).catch((error: unknown) => {
      this.addError('', 'transport', `Failed to send ${String(command.type)}: ${String(error)}`);
    });
  }

  private sendPrompt(message: string): void {
    void this.send({ type: 'prompt', id: this.newId('intent'), message });
  }

  private cancelPiRequest(piRequestId: string): void {
    if (piRequestId) void this.send(cancelledResponse(piRequestId));
  }

  private removePending(sessionId: string, requestId: string): PendingRequest | undefined {
    const queue = this.pending[sessionId] ?? [];
    const found = queue.find((r) => r.requestId === requestId);
    if (!found) return undefined;
    const rest = queue.filter((r) => r !== found);
    const { [sessionId]: _old, ...others } = this.pending;
    this.pending = rest.length > 0 ? { ...others, [sessionId]: rest } : others;
    return found;
  }

  private clearStates(sessionId: string): void {
    const kept: Record<string, ChannelState> = {};
    for (const [key, value] of Object.entries(this.states)) {
      if (value.sessionId !== sessionId) kept[key] = value;
    }
    this.states = kept;
  }

  private allowRequest(sessionId: string): boolean {
    const now = Date.now();
    const recent = (this.requestTimes.get(sessionId) ?? []).filter((t) => now - t < REQUEST_RATE_WINDOW_MS);
    if (recent.length >= MAX_REQUESTS_PER_WINDOW) {
      this.requestTimes.set(sessionId, recent);
      return false;
    }
    recent.push(now);
    this.requestTimes.set(sessionId, recent);
    return true;
  }

  private addError(sessionId: string, ch: string, message: string): void {
    const error: ProducerError = { id: ++this.nextErrorId, sessionId, ch, message, at: Date.now() };
    this.errors = [...this.errors, error].slice(-MAX_PRODUCER_ERRORS);
  }
}

const cancelledResponse = (id: string) => ({ type: 'extension_ui_response', id, cancelled: true });
const asString = (value: unknown): string | undefined => (typeof value === 'string' ? value : undefined);

export const channels = new ChannelsStore();
export type { ChannelsStore };
