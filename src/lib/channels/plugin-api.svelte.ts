/**
 * The channel API that PiPan plugins reach through `window.pipan.channel(id)`. It is a thin
 * layer over the channel store: subscribe to a topic, render a request, send an intent.
 */
import { untrack } from 'svelte';
import { channels, type ChannelState } from '#lib/stores/channels.svelte.ts';

export type Unsubscribe = () => void;

/** What a subscriber sees for a topic. */
export interface TopicSnapshot {
  primitive: string;
  topic: string;
  data: unknown;
  updatedAt: number;
}

/** Everything a custom request renderer needs to show the request and finish it. */
export interface ChannelRequestContext {
  ch: string;
  kind: string;
  payload: unknown;
  /** Sends the answer to the producer and closes the request. */
  answer: (payload: unknown) => Promise<void>;
  /** Cancels the request. The producer gets no answer. */
  cancel: () => Promise<void>;
}

/** Renders a request into `container`. May return a cleanup function. */
export type ChannelRequestHandler = (container: HTMLElement, request: ChannelRequestContext) => void | (() => void);

/** A log message the plugin can render in the transcript. */
export interface ChannelMessage {
  ch: string;
  name: string;
  data: unknown;
  content?: unknown;
  timestamp?: string;
}

/** Renders a log message into `container`. May return a cleanup function. */
export type ChannelMessageRenderer = (message: ChannelMessage, container: HTMLElement) => void | (() => void);

export interface ChannelApi {
  /**
   * Calls `callback` with the topic's latest state for the session being viewed, now if there is
   * one, then on every change. `null` means nothing is published, for example after the user
   * switches to a session without that topic.
   */
  subscribe(topic: string, callback: (snapshot: TopicSnapshot | null) => void): Unsubscribe;
  /**
   * Takes over rendering of requests of this kind from this channel. Requests of other kinds, and
   * of this kind from other channels, are unaffected.
   */
  onRequest(kind: string, handler: ChannelRequestHandler): Unsubscribe;
  /** Sends an intent to the producer of the viewed session. Resolves false when no producer can receive it. */
  intent(name: string, payload?: unknown): Promise<boolean>;
}

const handlerKey = (ch: string, kind: string) => `${ch}\u0000${kind}`;

class ChannelPluginHost {
  /** Request handlers by channel and kind. */
  handlers = $state.raw<Record<string, ChannelRequestHandler>>({});
  private activeSession: () => string = () => '';

  /** Tells the API which session the user is viewing. */
  configure(options: { activeSession: () => string }): void {
    this.activeSession = options.activeSession;
  }

  requestHandler(ch: string, kind: string): ChannelRequestHandler | undefined {
    return this.handlers[handlerKey(ch, kind)];
  }

  channel(ch: string): ChannelApi {
    const findState = (topic: string): ChannelState | undefined => {
      const session = this.activeSession();
      return channels.statesFor(session).find((s) => s.ch === ch && s.topic === topic);
    };

    return {
      subscribe: (topic, callback) => {
        let previous: ChannelState | undefined | null = null;
        let first = true;
        // An effect root keeps the subscription alive outside any component.
        return $effect.root(() => {
          $effect(() => {
            const state = findState(topic);
            // Skip repeats so a callback fires once per change, and once at the start.
            if (!first && state === previous) return;
            first = false;
            previous = state;
            const snapshot = state
              ? { primitive: state.primitive, topic: state.topic, data: state.data, updatedAt: state.updatedAt }
              : null;
            // The callback is plugin code. Whatever it reads must not become a dependency here.
            untrackCall(() => callback(snapshot));
          });
        });
      },

      onRequest: (kind, handler) => {
        const key = handlerKey(ch, kind);
        this.handlers = { ...this.handlers, [key]: handler };
        return () => {
          if (this.handlers[key] !== handler) return;
          const { [key]: _removed, ...rest } = this.handlers;
          this.handlers = rest;
        };
      },

      intent: (name, payload) => channels.sendIntent(this.activeSession(), ch, name, payload ?? {})
    };
  }
}

function untrackCall(fn: () => void): void {
  untrack(() => {
    try {
      fn();
    } catch (error) {
      // A throwing subscriber must not break the others or the store.
      console.error('[pipan] channel subscriber threw', error);
    }
  });
}

export const channelPlugins = new ChannelPluginHost();
