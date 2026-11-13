// Extension notifications (`ctx.ui.notify`). Pi sends them as fire-and-forget events. They are
// shown as toasts and kept in a short per-session list, and they never enter the transcript.

export type NotifyLevel = 'info' | 'warning' | 'error';

export interface Notification {
  id: number;
  sessionId: string;
  level: NotifyLevel;
  message: string;
  at: number;
}

/** Toasts on screen at once. The oldest leaves first. */
export const MAX_TOASTS = 5;
/** Notifications kept per session for the debug view. */
export const MAX_HISTORY = 100;
/** How long a toast stays. Errors stay until dismissed. */
export const TOAST_MS = 6000;
/** Longest message kept. */
export const MAX_MESSAGE_CHARS = 2000;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const levelOf = (value: unknown): NotifyLevel =>
  value === 'warning' || value === 'error' ? value : 'info';

class NotificationStore {
  /** Toasts on screen, oldest first. */
  toasts = $state.raw<Notification[]>([]);
  /** Every notification per session, oldest first, capped. */
  history = $state.raw<Record<string, Notification[]>>({});

  private nextId = 0;
  private timers = new Map<number, ReturnType<typeof setTimeout>>();

  /** Handles one Pi event. Returns true when it was a notification. */
  handleEvent(sessionId: string, event: unknown): boolean {
    if (!isRecord(event) || event.type !== 'extension_ui_request' || event.method !== 'notify') return false;
    let message = typeof event.message === 'string' ? event.message.trim() : '';
    if (message === '') return true;
    if (message.length > MAX_MESSAGE_CHARS) message = `${message.slice(0, MAX_MESSAGE_CHARS)}...`;
    this.add({ id: this.nextId++, sessionId, level: levelOf(event.notifyType), message, at: Date.now() });
    return true;
  }

  private add(item: Notification): void {
    const past = this.history[item.sessionId] ?? [];
    this.history = { ...this.history, [item.sessionId]: [...past, item].slice(-MAX_HISTORY) };

    const shown = [...this.toasts, item];
    for (const dropped of shown.slice(0, Math.max(0, shown.length - MAX_TOASTS))) this.clearTimer(dropped.id);
    this.toasts = shown.slice(-MAX_TOASTS);
    if (item.level !== 'error') {
      this.timers.set(
        item.id,
        setTimeout(() => this.dismiss(item.id), TOAST_MS)
      );
    }
  }

  dismiss(id: number): void {
    this.clearTimer(id);
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  historyFor(sessionId: string): Notification[] {
    return this.history[sessionId] ?? [];
  }

  /** Forgets everything stored for a deleted session. */
  forgetSession(sessionId: string): void {
    for (const t of this.toasts.filter((x) => x.sessionId === sessionId)) this.dismiss(t.id);
    const { [sessionId]: _gone, ...rest } = this.history;
    this.history = rest;
  }

  private clearTimer(id: number): void {
    const timer = this.timers.get(id);
    if (timer) clearTimeout(timer);
    this.timers.delete(id);
  }
}

export const notifications = new NotificationStore();
