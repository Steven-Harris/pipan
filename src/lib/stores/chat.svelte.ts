import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  ChatMessage,
  ContentBlock,
  PiModelInfo,
  PiRawMessage,
  PiRpcCommand,
  PiRpcEvent,
  PiSessionStatus,
  PiStderrPayload,
  SessionHistoryEntry,
  StreamingBehavior,
  TextContentBlock,
  ThinkingContentBlock,
  ToolCallContentBlock
} from '#lib/types/pi.ts';

function makeId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `id-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

function extractTextFromUserContent(content: unknown): string {
  if (typeof content === 'string') {
    return content;
  }
  if (Array.isArray(content)) {
    return content
      .map((item) => {
        if (typeof item === 'string') return item;
        if (item && typeof item === 'object' && 'text' in item && typeof item.text === 'string') {
          return item.text;
        }
        return '';
      })
      .filter(Boolean)
      .join('\n');
  }
  return '';
}

function extractToolResultText(
  resultObj?: { content?: Array<{ type?: string; text?: string; [key: string]: unknown }> } | null
): string {
  if (!resultObj || !Array.isArray(resultObj.content)) {
    return '';
  }
  return resultObj.content
    .map((part) => {
      if (typeof part?.text === 'string') {
        return part.text;
      }
      if (part?.type === 'image') {
        return '[Image output]';
      }
      return '';
    })
    .filter(Boolean)
    .join('\n');
}

function formatWorkspaceLabel(mode: 'headless' | 'directory', cwd: string | null): string {
  if (mode === 'headless' || !cwd) {
    return 'Headless Sandbox';
  }
  const cleaned = cwd.replace(/^\\\\\?\\/, '');
  const sep = cleaned.includes('\\') ? '\\' : '/';
  const parts = cleaned.split(/[\\/]+/).filter(Boolean);
  if (parts.length <= 2) return cleaned;
  return `…${sep}${parts.slice(-2).join(sep)}`;
}

class ChatStore {
  sessionStatus = $state<PiSessionStatus>({
    state: 'disconnected',
    mode: 'headless',
    cwd: null,
    pid: null,
    error: null
  });

  messages = $state<ChatMessage[]>([]);
  isAgentRunning = $state<boolean>(false);
  steeringQueue = $state<string[]>([]);
  followUpQueue = $state<string[]>([]);
  stderrLines = $state<PiStderrPayload[]>([]);
  lastError = $state<string | null>(null);
  modelInfo = $state<PiModelInfo | null>(null);
  isCompacting = $state<boolean>(false);

  // UI Shell state (Phase 1)
  sidebarOpen = $state<boolean>(true);
  drawerMode = $state<'split' | 'expanded' | 'collapsed'>('split');
  selectedNodeId = $state<string | null>(null);
  sessionHistory = $state<SessionHistoryEntry[]>([]);
  activeSessionId = $state<string | null>(null);

  private initialized = false;
  private unlisteners: UnlistenFn[] = [];
  // Maps `contentIndex` of the currently streaming assistant message to its index in `msg.blocks`
  private activeBlockIndexMap = new Map<number, number>();

  async init(): Promise<void> {
    if (this.initialized) return;
    this.initialized = true;

    const unlistenEvent = await listen<PiRpcEvent>('pi:event', (event) => {
      this.handlePiEvent(event.payload);
      this.syncActiveSessionHistory();
    });

    const unlistenStatus = await listen<PiSessionStatus>('pi:status', (event) => {
      this.sessionStatus = event.payload;
      if (event.payload.error) {
        this.lastError = event.payload.error;
      }
      if (event.payload.state === 'disconnected' || event.payload.state === 'error') {
        this.isAgentRunning = false;
        this.finalizeStreamingMessages();
      }
      this.syncActiveSessionHistory();
    });

    const unlistenStderr = await listen<PiStderrPayload>('pi:stderr', (event) => {
      this.stderrLines = [...this.stderrLines.slice(-99), event.payload];
    });

    this.unlisteners.push(unlistenEvent, unlistenStatus, unlistenStderr);

    try {
      const status = await invoke<PiSessionStatus>('get_pi_status');
      this.sessionStatus = status;
      if (status.state === 'running') {
        this.recordSessionStart(status);
        await this.requestState();
      }
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
    }
  }

  destroy(): void {
    for (const unlisten of this.unlisteners) {
      unlisten();
    }
    this.unlisteners = [];
    this.initialized = false;
  }

  toggleSidebar(): void {
    this.sidebarOpen = !this.sidebarOpen;
  }

  setDrawerMode(mode: 'split' | 'expanded' | 'collapsed'): void {
    this.drawerMode = mode;
  }

  private recordSessionStart(status: PiSessionStatus): void {
    const id = makeId();
    this.activeSessionId = id;
    const entry: SessionHistoryEntry = {
      id,
      mode: status.mode,
      cwd: status.cwd,
      label: formatWorkspaceLabel(status.mode, status.cwd),
      startedAt: Date.now(),
      pid: status.pid,
      state: status.state,
      messageCount: 0,
      toolCallCount: 0
    };
    this.sessionHistory = [entry, ...this.sessionHistory.slice(0, 19)];
  }

  private syncActiveSessionHistory(): void {
    if (!this.activeSessionId) return;
    const idx = this.sessionHistory.findIndex((s) => s.id === this.activeSessionId);
    if (idx === -1) return;

    let toolCalls = 0;
    for (const msg of this.messages) {
      for (const b of msg.blocks) {
        if (b.type === 'tool_call') toolCalls += 1;
      }
    }

    const current = this.sessionHistory[idx];
    this.sessionHistory[idx] = {
      ...current,
      state: this.sessionStatus.state,
      pid: this.sessionStatus.pid,
      cwd: this.sessionStatus.cwd,
      label: formatWorkspaceLabel(this.sessionStatus.mode, this.sessionStatus.cwd),
      messageCount: this.messages.length,
      toolCallCount: toolCalls
    };
  }

  async startHeadless(): Promise<void> {
    this.lastError = null;
    this.stderrLines = [];
    this.messages = [];
    this.steeringQueue = [];
    this.followUpQueue = [];
    this.modelInfo = null;
    this.selectedNodeId = null;
    this.activeBlockIndexMap.clear();

    try {
      const status = await invoke<PiSessionStatus>('start_pi_session', { cwd: null });
      this.sessionStatus = status;
      this.recordSessionStart(status);
      await this.requestState();
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
    }
  }

  async startInDirectory(cwd: string): Promise<void> {
    this.lastError = null;
    this.stderrLines = [];
    this.messages = [];
    this.steeringQueue = [];
    this.followUpQueue = [];
    this.modelInfo = null;
    this.selectedNodeId = null;
    this.activeBlockIndexMap.clear();

    try {
      const status = await invoke<PiSessionStatus>('start_pi_session', { cwd });
      this.sessionStatus = status;
      this.recordSessionStart(status);
      await this.requestState();
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
    }
  }

  async stopSession(): Promise<void> {
    try {
      const status = await invoke<PiSessionStatus>('stop_pi_session');
      this.sessionStatus = status;
      this.isAgentRunning = false;
      this.steeringQueue = [];
      this.followUpQueue = [];
      this.finalizeStreamingMessages();
      this.syncActiveSessionHistory();
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
    }
  }

  async sendCommand(command: PiRpcCommand): Promise<void> {
    try {
      await invoke('send_pi_command', { command });
    } catch (err) {
      this.lastError = err instanceof Error ? err.message : String(err);
      throw err;
    }
  }

  async requestState(): Promise<void> {
    if (this.sessionStatus.state !== 'running') return;
    try {
      await this.sendCommand({ id: makeId(), type: 'get_state' });
    } catch {
      // Handled in sendCommand
    }
  }

  async sendPrompt(text: string, streamingBehavior?: StreamingBehavior): Promise<void> {
    const trimmed = text.trim();
    if (!trimmed) return;
    if (this.sessionStatus.state !== 'running') {
      this.lastError = 'Start a Headless or Folder session before sending a message.';
      return;
    }

    this.lastError = null;

    if (!this.isAgentRunning) {
      // Optimistically show user message; reconciled when `message_start` (role: "user") arrives
      this.messages.push({
        id: makeId(),
        role: 'user',
        timestamp: Date.now(),
        blocks: [{ type: 'text', text: trimmed, isComplete: true }],
        isOptimistic: true
      });

      await this.sendCommand({
        id: makeId(),
        type: 'prompt',
        message: trimmed
      });
    } else {
      const behavior: StreamingBehavior = streamingBehavior ?? 'steer';
      await this.sendCommand({
        id: makeId(),
        type: 'prompt',
        message: trimmed,
        streamingBehavior: behavior
      });
    }
  }

  async abort(): Promise<void> {
    if (this.sessionStatus.state !== 'running') return;
    try {
      await this.sendCommand({ id: makeId(), type: 'clear_queue' });
      await this.sendCommand({ id: makeId(), type: 'abort' });
    } catch {
      // Error surfaced via lastError
    }
  }

  async newSession(): Promise<void> {
    if (this.sessionStatus.state !== 'running') {
      this.messages = [];
      return;
    }
    this.lastError = null;
    this.messages = [];
    this.steeringQueue = [];
    this.followUpQueue = [];
    this.selectedNodeId = null;
    this.activeBlockIndexMap.clear();
    this.recordSessionStart(this.sessionStatus);

    try {
      await this.sendCommand({ id: makeId(), type: 'new_session' });
      await this.requestState();
    } catch {
      // Error surfaced via lastError
    }
  }

  dismissError(): void {
    this.lastError = null;
  }

  clearStderr(): void {
    this.stderrLines = [];
  }

  // ==========================================================================
  // Internal Event Stream Reconstruction
  // ==========================================================================

  private handlePiEvent(event: PiRpcEvent): void {
    if (!event || typeof event.type !== 'string') return;

    switch (event.type) {
      case 'agent_start': {
        this.isAgentRunning = true;
        break;
      }

      case 'agent_end':
      case 'agent_settled': {
        this.isAgentRunning = false;
        this.finalizeStreamingMessages();
        break;
      }

      case 'message_start': {
        this.handleMessageStart(event.message);
        break;
      }

      case 'message_update': {
        this.handleMessageUpdate(event);
        break;
      }

      case 'message_end': {
        this.handleMessageEnd(event.message);
        break;
      }

      case 'tool_execution_start': {
        this.handleToolExecutionStart(event);
        break;
      }

      case 'tool_execution_update': {
        this.handleToolExecutionUpdate(event);
        break;
      }

      case 'tool_execution_end': {
        this.handleToolExecutionEnd(event);
        break;
      }

      case 'queue_update': {
        this.steeringQueue = Array.isArray(event.steering) ? event.steering : [];
        this.followUpQueue = Array.isArray(event.followUp) ? event.followUp : [];
        break;
      }

      case 'compaction_start': {
        this.isCompacting = true;
        break;
      }

      case 'compaction_end': {
        this.isCompacting = false;
        break;
      }

      case 'extension_error': {
        const errText =
          typeof event.error === 'string'
            ? event.error
            : 'An extension error occurred in the Pi session.';
        this.lastError = errText;
        break;
      }

      case 'response': {
        this.handleResponseEvent(event);
        break;
      }
    }
  }

  private handleMessageStart(rawMsg?: PiRawMessage): void {
    if (!rawMsg) return;

    if (rawMsg.role === 'user') {
      const text = extractTextFromUserContent(rawMsg.content);
      // Check if there is an optimistic user message to reconcile
      for (let i = this.messages.length - 1; i >= 0; i--) {
        const m = this.messages[i];
        if (m.role === 'user' && m.isOptimistic) {
          m.isOptimistic = false;
          m.timestamp = rawMsg.timestamp ?? m.timestamp;
          if (text) {
            m.blocks = [{ type: 'text', text, isComplete: true }];
          }
          return;
        }
      }

      this.messages.push({
        id: rawMsg.id ?? makeId(),
        role: 'user',
        timestamp: rawMsg.timestamp ?? Date.now(),
        blocks: [{ type: 'text', text: text || '(empty message)', isComplete: true }]
      });
      return;
    }

    if (rawMsg.role === 'assistant') {
      this.activeBlockIndexMap.clear();
      this.messages.push({
        id: rawMsg.id ?? makeId(),
        role: 'assistant',
        timestamp: rawMsg.timestamp ?? Date.now(),
        blocks: [],
        isStreaming: true,
        usage: rawMsg.usage
      });
    }
  }

  private getOrCreateStreamingAssistantMessage(): ChatMessage {
    const last = this.messages[this.messages.length - 1];
    if (last && last.role === 'assistant' && last.isStreaming) {
      return last;
    }
    this.activeBlockIndexMap.clear();
    const created: ChatMessage = {
      id: makeId(),
      role: 'assistant',
      timestamp: Date.now(),
      blocks: [],
      isStreaming: true
    };
    this.messages.push(created);
    return created;
  }

  private getOrCreateBlockAtContentIndex<T extends ContentBlock>(
    msg: ChatMessage,
    contentIndex: number,
    expectedType: T['type'],
    factory: () => T
  ): T {
    const mappedIdx = this.activeBlockIndexMap.get(contentIndex);
    if (mappedIdx !== undefined && msg.blocks[mappedIdx]?.type === expectedType) {
      return msg.blocks[mappedIdx] as T;
    }

    // Fallback: if no mapping yet, create block and record index
    const newBlock = factory();
    msg.blocks.push(newBlock);
    const newIdx = msg.blocks.length - 1;
    this.activeBlockIndexMap.set(contentIndex, newIdx);
    return msg.blocks[newIdx] as T;
  }

  private handleMessageUpdate(event: PiRpcEvent): void {
    const sub = event.assistantMessageEvent;
    if (!sub) return;

    const msg = this.getOrCreateStreamingAssistantMessage();
    const contentIndex = typeof sub.contentIndex === 'number' ? sub.contentIndex : 0;

    if (event.message?.usage) {
      msg.usage = event.message.usage;
    }

    switch (sub.type) {
      case 'text_start': {
        this.getOrCreateBlockAtContentIndex<TextContentBlock>(msg, contentIndex, 'text', () => ({
          type: 'text',
          text: '',
          isComplete: false
        }));
        break;
      }

      case 'text_delta': {
        const block = this.getOrCreateBlockAtContentIndex<TextContentBlock>(
          msg,
          contentIndex,
          'text',
          () => ({
            type: 'text',
            text: '',
            isComplete: false
          })
        );
        if (typeof sub.delta === 'string') {
          block.text += sub.delta;
        }
        break;
      }

      case 'text_end': {
        const block = this.getOrCreateBlockAtContentIndex<TextContentBlock>(
          msg,
          contentIndex,
          'text',
          () => ({
            type: 'text',
            text: '',
            isComplete: true
          })
        );
        if (typeof sub.content === 'string') {
          block.text = sub.content;
        }
        block.isComplete = true;
        break;
      }

      case 'thinking_start': {
        this.getOrCreateBlockAtContentIndex<ThinkingContentBlock>(
          msg,
          contentIndex,
          'thinking',
          () => ({
            type: 'thinking',
            thinking: '',
            isComplete: false
          })
        );
        break;
      }

      case 'thinking_delta': {
        const block = this.getOrCreateBlockAtContentIndex<ThinkingContentBlock>(
          msg,
          contentIndex,
          'thinking',
          () => ({
            type: 'thinking',
            thinking: '',
            isComplete: false
          })
        );
        if (typeof sub.delta === 'string') {
          block.thinking += sub.delta;
        }
        break;
      }

      case 'thinking_end': {
        const block = this.getOrCreateBlockAtContentIndex<ThinkingContentBlock>(
          msg,
          contentIndex,
          'thinking',
          () => ({
            type: 'thinking',
            thinking: '',
            isComplete: true
          })
        );
        if (typeof sub.content === 'string') {
          block.thinking = sub.content;
        }
        block.isComplete = true;
        break;
      }

      case 'toolcall_start': {
        const partialContent = Array.isArray(sub.partial?.content)
          ? (sub.partial.content[contentIndex] as Record<string, unknown> | undefined)
          : undefined;
        const id =
          sub.toolCall?.id ??
          (typeof partialContent?.id === 'string' ? partialContent.id : undefined) ??
          makeId();
        const name =
          sub.toolCall?.name ??
          (typeof partialContent?.name === 'string' ? partialContent.name : undefined) ??
          'tool';

        this.getOrCreateBlockAtContentIndex<ToolCallContentBlock>(
          msg,
          contentIndex,
          'tool_call',
          () => ({
            type: 'tool_call',
            id,
            name,
            argsJson: '',
            status: 'streaming_args',
            startedAt: Date.now()
          })
        );
        break;
      }

      case 'toolcall_delta': {
        const partialContent = Array.isArray(sub.partial?.content)
          ? (sub.partial.content[contentIndex] as Record<string, unknown> | undefined)
          : undefined;
        const block = this.getOrCreateBlockAtContentIndex<ToolCallContentBlock>(
          msg,
          contentIndex,
          'tool_call',
          () => ({
            type: 'tool_call',
            id:
              (typeof partialContent?.id === 'string' ? partialContent.id : undefined) ?? makeId(),
            name:
              (typeof partialContent?.name === 'string' ? partialContent.name : undefined) ??
              'tool',
            argsJson: '',
            status: 'streaming_args',
            startedAt: Date.now()
          })
        );
        if (typeof partialContent?.id === 'string' && partialContent.id) {
          block.id = partialContent.id;
        }
        if (typeof partialContent?.name === 'string' && partialContent.name) {
          block.name = partialContent.name;
        }
        if (typeof sub.delta === 'string') {
          block.argsJson += sub.delta;
        }
        break;
      }

      case 'toolcall_end': {
        const block = this.getOrCreateBlockAtContentIndex<ToolCallContentBlock>(
          msg,
          contentIndex,
          'tool_call',
          () => ({
            type: 'tool_call',
            id: sub.toolCall?.id ?? makeId(),
            name: sub.toolCall?.name ?? 'tool',
            argsJson: '',
            status: 'streaming_args',
            startedAt: Date.now()
          })
        );
        if (sub.toolCall?.id) {
          block.id = sub.toolCall.id;
        }
        if (sub.toolCall?.name) {
          block.name = sub.toolCall.name;
        }
        if (sub.toolCall?.arguments) {
          block.args = sub.toolCall.arguments;
          block.argsJson = JSON.stringify(sub.toolCall.arguments, null, 2);
        } else if (block.argsJson) {
          try {
            block.args = JSON.parse(block.argsJson);
          } catch {
            // Keep raw argsJson
          }
        }
        break;
      }
    }
  }

  private handleMessageEnd(rawMsg?: PiRawMessage): void {
    if (!rawMsg || rawMsg.role !== 'assistant') return;

    const last = this.messages[this.messages.length - 1];
    if (!last || last.role !== 'assistant') return;

    last.isStreaming = false;
    if (rawMsg.stopReason) {
      last.stopReason = rawMsg.stopReason;
    }
    if (rawMsg.errorMessage) {
      last.errorMessage = rawMsg.errorMessage;
      this.lastError = rawMsg.errorMessage;
    }
    if (rawMsg.usage) {
      last.usage = rawMsg.usage;
    }

    // If no blocks were populated via deltas, hydrate from rawMsg.content
    if (last.blocks.length === 0 && Array.isArray(rawMsg.content)) {
      for (const item of rawMsg.content) {
        if (item.type === 'text' && typeof item.text === 'string') {
          last.blocks.push({
            type: 'text',
            text: item.text,
            isComplete: true
          });
        } else if (item.type === 'thinking' && typeof item.thinking === 'string') {
          last.blocks.push({
            type: 'thinking',
            thinking: item.thinking,
            isComplete: true
          });
        } else if (item.type === 'toolCall') {
          const args =
            item.arguments && typeof item.arguments === 'object'
              ? (item.arguments as Record<string, unknown>)
              : undefined;
          last.blocks.push({
            type: 'tool_call',
            id: typeof item.id === 'string' ? item.id : makeId(),
            name: typeof item.name === 'string' ? item.name : 'tool',
            args,
            argsJson: args ? JSON.stringify(args, null, 2) : '',
            status: 'running'
          });
        }
      }
    }

    // Mark text/thinking blocks complete
    for (const block of last.blocks) {
      if (block.type === 'text' || block.type === 'thinking') {
        block.isComplete = true;
      }
    }

    this.activeBlockIndexMap.clear();
  }

  private findToolCallBlock(toolCallId?: string): ToolCallContentBlock | null {
    if (!toolCallId) return null;
    for (let i = this.messages.length - 1; i >= 0; i--) {
      const msg = this.messages[i];
      if (msg.role !== 'assistant') continue;
      for (let j = msg.blocks.length - 1; j >= 0; j--) {
        const block = msg.blocks[j];
        if (block.type === 'tool_call' && block.id === toolCallId) {
          return block;
        }
      }
    }
    return null;
  }

  private handleToolExecutionStart(event: PiRpcEvent): void {
    const toolCallId = event.toolCallId ?? makeId();
    let block = this.findToolCallBlock(toolCallId);

    if (!block) {
      // Attach to most recent assistant message or create one
      let targetMsg = this.messages[this.messages.length - 1];
      if (!targetMsg || targetMsg.role !== 'assistant') {
        targetMsg = this.getOrCreateStreamingAssistantMessage();
      }
      const newBlock: ToolCallContentBlock = {
        type: 'tool_call',
        id: toolCallId,
        name: event.toolName ?? 'tool',
        args: event.args,
        argsJson: event.args ? JSON.stringify(event.args, null, 2) : '',
        status: 'running',
        startedAt: Date.now()
      };
      targetMsg.blocks.push(newBlock);
      block = targetMsg.blocks[targetMsg.blocks.length - 1] as ToolCallContentBlock;
    } else {
      block.status = 'running';
      block.startedAt = block.startedAt ?? Date.now();
      if (event.toolName) {
        block.name = event.toolName;
      }
      if (event.args) {
        block.args = event.args;
        block.argsJson = JSON.stringify(event.args, null, 2);
      }
    }
  }

  private handleToolExecutionUpdate(event: PiRpcEvent): void {
    const block = this.findToolCallBlock(event.toolCallId);
    if (!block) return;

    block.status = 'running';
    const partialText = extractToolResultText(event.partialResult);
    if (partialText) {
      block.partialOutput = partialText;
    }
  }

  private handleToolExecutionEnd(event: PiRpcEvent): void {
    const block = this.findToolCallBlock(event.toolCallId);
    if (!block) return;

    const isError = Boolean(event.isError);
    block.status = isError ? 'error' : 'completed';
    block.isError = isError;
    if (block.startedAt) {
      block.durationMs = Math.max(0, Date.now() - block.startedAt);
    }

    const finalOutput = extractToolResultText(event.result);
    if (finalOutput) {
      block.resultOutput = finalOutput;
    } else if (block.partialOutput) {
      block.resultOutput = block.partialOutput;
    }
  }

  private handleResponseEvent(event: PiRpcEvent): void {
    if (event.success === false) {
      this.lastError = event.error ?? `Pi RPC command '${event.command ?? 'unknown'}' failed`;
      return;
    }

    if (event.command === 'get_state' && event.data) {
      const data = event.data;
      const rawModel = data.model as Record<string, unknown> | undefined;
      this.modelInfo = {
        id: typeof rawModel?.id === 'string' ? rawModel.id : undefined,
        name: typeof rawModel?.name === 'string' ? rawModel.name : undefined,
        provider: typeof rawModel?.provider === 'string' ? rawModel.provider : undefined,
        thinkingLevel: typeof data.thinkingLevel === 'string' ? data.thinkingLevel : undefined
      };
      if (typeof data.isStreaming === 'boolean') {
        this.isAgentRunning = data.isStreaming;
      }
      if (typeof data.isCompacting === 'boolean') {
        this.isCompacting = data.isCompacting;
      }
    }
  }

  private finalizeStreamingMessages(): void {
    for (const msg of this.messages) {
      if (msg.isStreaming) {
        msg.isStreaming = false;
      }
      for (const block of msg.blocks) {
        if (block.type === 'text' || block.type === 'thinking') {
          block.isComplete = true;
        }
      }
    }
    this.activeBlockIndexMap.clear();
  }
}

export const chatStore = new ChatStore();
