<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    Send,
    Compass,
    Clock,
    Square,
    PowerOff,
    Paperclip,
    Wand2,
    Sparkles,
    FileCode,
    X,
    Upload
  } from 'lucide-svelte';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import type {
    PiSkillItem,
    PromptAttachment,
    StreamingBehavior
  } from '#lib/types/pi.ts';
  import ModelSelector from './ModelSelector.svelte';
  import SkillPickerPopover from './SkillPickerPopover.svelte';

  let draft = $state('');
  let textareaEl = $state<HTMLTextAreaElement | null>(null);
  let skillToolbarWrapperEl = $state<HTMLDivElement | null>(null);
  let skillPopoverRef = $state<ReturnType<typeof SkillPickerPopover> | null>(null);

  // Skills & slash command state
  let selectedSkills = $state<PiSkillItem[]>([]);
  let isSkillPopoverOpen = $state(false);
  let activeSlashMatch = $state<{ start: number; end: number; query: string } | null>(null);

  // File & Image Attachments state (shared on chatStore so FileExplorerPanel can attach directly)
  let isDraggingOver = $state(false);
  let isPickingFiles = $state(false);

  const attachments = $derived(chatStore.promptAttachments);
  const isConnected = $derived(chatStore.sessionStatus.state === 'running');
  const canSend = $derived(
    isConnected &&
      (draft.trim().length > 0 || selectedSkills.length > 0 || attachments.length > 0)
  );

  const imageAttachments = $derived(attachments.filter((a) => a.kind === 'image' && a.data));
  const fileAttachments = $derived(attachments.filter((a) => a.kind !== 'image'));
  const selectedSkillNames = $derived(selectedSkills.map((s) => s.name));

  function makeAttachmentId(): string {
    return `att-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  }

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    textareaEl.style.height = `${Math.min(textareaEl.scrollHeight, 180)}px`;
  }

  function detectSlashTrigger() {
    if (!textareaEl) {
      activeSlashMatch = null;
      return;
    }
    const cursor = textareaEl.selectionStart ?? draft.length;
    const beforeCursor = draft.slice(0, cursor);
    const match = /(?:^|\s)(\/[a-zA-Z0-9_:.-]*)$/.exec(beforeCursor);
    if (match) {
      const token = match[1];
      const start = cursor - token.length;
      activeSlashMatch = { start, end: cursor, query: token };
    } else {
      activeSlashMatch = null;
    }
  }

  function handleInput() {
    autoResize();
    detectSlashTrigger();
  }

  function handleSelectSkill(skill: PiSkillItem) {
    if (activeSlashMatch) {
      const before = draft.slice(0, activeSlashMatch.start);
      const after = draft.slice(activeSlashMatch.end);
      draft = `${before}${after}`.replace(/^\s+/, '');
      activeSlashMatch = null;
    }
    if (!selectedSkills.some((s) => s.name === skill.name && s.source === skill.source)) {
      selectedSkills = [...selectedSkills, skill];
    }
    isSkillPopoverOpen = false;
    queueMicrotask(() => {
      autoResize();
      textareaEl?.focus();
    });
  }

  function removeSkillPill(skillName: string) {
    selectedSkills = selectedSkills.filter((s) => s.name !== skillName);
  }

  function removeAttachment(id: string) {
    chatStore.removePromptAttachment(id);
  }

  async function addAttachmentsFromPaths(paths: string[]) {
    for (const rawPath of paths) {
      await chatStore.attachFileFromPath(rawPath);
    }
  }

  function isImageExtension(name: string, mime: string): boolean {
    if (mime.startsWith('image/')) return true;
    const ext = name.split('.').pop()?.toLowerCase() ?? '';
    return ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'].includes(ext);
  }

  function inferImageMime(name: string, fallbackMime: string): string {
    if (fallbackMime && fallbackMime.startsWith('image/')) return fallbackMime;
    const ext = name.split('.').pop()?.toLowerCase() ?? '';
    if (ext === 'jpg' || ext === 'jpeg') return 'image/jpeg';
    if (ext === 'gif') return 'image/gif';
    if (ext === 'webp') return 'image/webp';
    if (ext === 'bmp') return 'image/bmp';
    return 'image/png';
  }

  function readBrowserFileAsDataUrl(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(typeof reader.result === 'string' ? reader.result : '');
      reader.onerror = () => reject(new Error(`Failed to read image file ${file.name}`));
      reader.readAsDataURL(file);
    });
  }

  function readBrowserFileAsText(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(typeof reader.result === 'string' ? reader.result : '');
      reader.onerror = () => reject(new Error(`Failed to read text file ${file.name}`));
      reader.readAsText(file);
    });
  }

  async function addAttachmentsFromBrowserFiles(fileList: FileList) {
    for (const file of Array.from(fileList)) {
      const maybePath = (file as File & { path?: string }).path;
      if (typeof maybePath === 'string' && maybePath.trim()) {
        await addAttachmentsFromPaths([maybePath]);
        continue;
      }

      try {
        if (isImageExtension(file.name, file.type)) {
          const dataUrl = await readBrowserFileAsDataUrl(file);
          const commaIdx = dataUrl.indexOf(',');
          const base64 = commaIdx !== -1 ? dataUrl.slice(commaIdx + 1) : dataUrl;
          const mimeType = inferImageMime(file.name, file.type);
          chatStore.addPromptAttachment({
            id: makeAttachmentId(),
            kind: 'image',
            type: 'image',
            name: file.name,
            path: file.name,
            insideWorkspace: false,
            mimeType,
            sizeBytes: file.size,
            data: base64
          });
        } else {
          const textContent = file.size <= 256 * 1024 ? await readBrowserFileAsText(file) : undefined;
          chatStore.addPromptAttachment({
            id: makeAttachmentId(),
            kind: 'file',
            type: 'file',
            name: file.name,
            path: file.name,
            insideWorkspace: false,
            mimeType: file.type || 'text/plain',
            sizeBytes: file.size,
            textContent
          });
        }
      } catch (err) {
        chatStore.lastError = err instanceof Error ? err.message : String(err);
      }
    }
  }

  async function handlePickFiles() {
    if (isPickingFiles) return;
    isPickingFiles = true;
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        title: 'Attach Files or Images to Prompt'
      });
      if (Array.isArray(selected) && selected.length > 0) {
        await addAttachmentsFromPaths(selected);
      }
    } catch (err) {
      chatStore.lastError = err instanceof Error ? err.message : String(err);
    } finally {
      isPickingFiles = false;
    }
  }

  function handleHtmlDragOver(e: DragEvent) {
    e.preventDefault();
    isDraggingOver = true;
  }

  function handleHtmlDragLeave(e: DragEvent) {
    e.preventDefault();
    isDraggingOver = false;
  }

  async function handleHtmlDrop(e: DragEvent) {
    e.preventDefault();
    isDraggingOver = false;
    const pipanFilePath = e.dataTransfer?.getData('application/x-pipan-file-path')?.trim();
    if (pipanFilePath) {
      await addAttachmentsFromPaths([pipanFilePath]);
      return;
    }
    if (e.dataTransfer?.files && e.dataTransfer.files.length > 0) {
      await addAttachmentsFromBrowserFiles(e.dataTransfer.files);
    }
  }

  onMount(() => {
    let unlistenDragDrop: (() => void) | undefined;
    void (async () => {
      try {
        unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
          const payload = event.payload;
          if (payload.type === 'enter' || payload.type === 'over') {
            isDraggingOver = true;
          } else if (payload.type === 'leave') {
            isDraggingOver = false;
          } else if (payload.type === 'drop') {
            isDraggingOver = false;
            if (Array.isArray(payload.paths) && payload.paths.length > 0) {
              void addAttachmentsFromPaths(payload.paths);
            }
          }
        });
      } catch {
        // Non-Tauri browser preview fallback uses HTML5 drag-and-drop events
      }
    })();

    return () => {
      unlistenDragDrop?.();
    };
  });

  function buildFinalPromptText(userText: string): string {
    const prefixParts: string[] = [];

    for (const skill of selectedSkills) {
      const token = skill.source === 'skill' ? `/skill:${skill.name}` : `/${skill.name}`;
      if (!userText.includes(token)) {
        prefixParts.push(token);
      }
    }

    const fileRefParts: string[] = [];
    const inlineFileBlocks: string[] = [];

    for (const att of fileAttachments) {
      const refPath = att.relativePath || att.path || att.name;
      if (att.insideWorkspace || !att.textContent) {
        fileRefParts.push(`@${refPath}`);
      } else {
        const ext = att.name.includes('.') ? (att.name.split('.').pop() ?? '') : '';
        fileRefParts.push(`@${refPath}`);
        inlineFileBlocks.push(`\`\`\`${ext}\n// ${refPath}\n${att.textContent.trimEnd()}\n\`\`\``);
      }
    }

    const segments: string[] = [];
    if (prefixParts.length > 0) {
      segments.push(prefixParts.join(' '));
    }
    if (fileRefParts.length > 0) {
      segments.push(fileRefParts.join(' '));
    }
    if (userText) {
      segments.push(userText);
    }
    if (inlineFileBlocks.length > 0) {
      segments.push(inlineFileBlocks.join('\n\n'));
    }

    return segments.join('\n\n').trim();
  }

  async function submitMessage(behavior?: StreamingBehavior) {
    if (!canSend) return;
    const rawText = draft.trim();
    const finalMessage = buildFinalPromptText(rawText);
    const imagesPayload = imageAttachments
      .filter((img): img is PromptAttachment & { data: string } => Boolean(img.data))
      .map((img) => ({
        type: 'image' as const,
        data: img.data,
        mimeType: img.mimeType
      }));

    if (!finalMessage && imagesPayload.length === 0) return;

    draft = '';
    selectedSkills = [];
    chatStore.clearPromptAttachments();
    activeSlashMatch = null;
    isSkillPopoverOpen = false;
    if (textareaEl) {
      textareaEl.style.height = 'auto';
    }
    await chatStore.sendPrompt(finalMessage, behavior, imagesPayload);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.isComposing || e.keyCode === 229) return;

    const popoverVisible = isSkillPopoverOpen || activeSlashMatch !== null;
    if (popoverVisible && skillPopoverRef) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        skillPopoverRef.moveSelection(1);
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        skillPopoverRef.moveSelection(-1);
        return;
      }
      if ((e.key === 'Enter' || e.key === 'Tab') && activeSlashMatch !== null) {
        if (skillPopoverRef.confirmHighlighted()) {
          e.preventDefault();
          return;
        }
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        isSkillPopoverOpen = false;
        activeSlashMatch = null;
        return;
      }
    }

    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void submitMessage(chatStore.isAgentRunning ? 'steer' : undefined);
    }
  }

  function handleWindowClick(e: MouseEvent) {
    if (!isSkillPopoverOpen || !skillToolbarWrapperEl) return;
    if (!skillToolbarWrapperEl.contains(e.target as Node)) {
      isSkillPopoverOpen = false;
    }
  }
</script>

<svelte:window onclick={handleWindowClick} />

<div
  role="region"
  aria-label="Message composer"
  ondragover={handleHtmlDragOver}
  ondragleave={handleHtmlDragLeave}
  ondrop={handleHtmlDrop}
  class="relative border-t border-zinc-800/80 bg-zinc-900/70 px-4 py-3 backdrop-blur-md transition {isDraggingOver
    ? 'ring-2 ring-inset ring-indigo-500/70 bg-indigo-950/30'
    : ''}"
>
  {#if isDraggingOver}
    <div class="pointer-events-none absolute inset-2 z-40 flex items-center justify-center rounded-xl border-2 border-dashed border-indigo-400/70 bg-zinc-950/85 text-xs font-medium text-indigo-200">
      <div class="flex items-center gap-2">
        <Upload class="h-4 w-4 text-indigo-400" />
        <span>Drop images or files to attach to your prompt</span>
      </div>
    </div>
  {/if}

  <div class="mx-auto max-w-4xl space-y-2">
    <!-- Active Steering / Follow-Up Queues -->
    {#if chatStore.steeringQueue.length > 0 || chatStore.followUpQueue.length > 0}
      <div class="flex flex-wrap items-center gap-1.5 text-xs">
        {#each chatStore.steeringQueue as item, i (`steer-${i}`)}
          <span class="inline-flex items-center gap-1 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-amber-300">
            <Compass class="h-3 w-3" />
            <span class="font-medium">Steer:</span>
            <span class="max-w-xs truncate">{item}</span>
          </span>
        {/each}
        {#each chatStore.followUpQueue as item, i (`follow-${i}`)}
          <span class="inline-flex items-center gap-1 rounded-md border border-sky-500/30 bg-sky-500/10 px-2 py-0.5 text-sky-300">
            <Clock class="h-3 w-3" />
            <span class="font-medium">Follow-up:</span>
            <span class="max-w-xs truncate">{item}</span>
          </span>
        {/each}
      </div>
    {/if}

    <!-- Selected Skill Pills, Image Thumbnails & File Chips -->
    {#if selectedSkills.length > 0 || imageAttachments.length > 0 || fileAttachments.length > 0}
      <div class="flex flex-wrap items-center gap-2 pt-0.5">
        <!-- Removable /skill:<name> pills -->
        {#each selectedSkills as skill (`${skill.source}:${skill.name}`)}
          {@const token = skill.source === 'skill' ? `/skill:${skill.name}` : `/${skill.name}`}
          <span
            class="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-xs font-mono text-indigo-200"
            title={skill.description || token}
          >
            <Sparkles class="h-3 w-3 shrink-0 text-indigo-400" />
            <span>{token}</span>
            <button
              type="button"
              onclick={() => removeSkillPill(skill.name)}
              aria-label={`Remove ${token}`}
              class="ml-0.5 rounded p-0.5 text-indigo-300 hover:bg-indigo-500/30 hover:text-white cursor-pointer"
            >
              <X class="h-3 w-3" />
            </button>
          </span>
        {/each}

        <!-- Image Thumbnails -->
        {#each imageAttachments as img (img.id)}
          <div class="group relative inline-flex items-center gap-2 rounded-lg border border-zinc-700/80 bg-zinc-900/90 p-1 pr-2 text-xs text-zinc-200">
            <img
              src={`data:${img.mimeType};base64,${img.data}`}
              alt={img.name}
              class="h-9 w-9 rounded object-cover border border-zinc-800"
            />
            <div class="flex flex-col min-w-0 max-w-[140px]">
              <span class="truncate font-medium text-zinc-100">{img.name}</span>
              <span class="text-[10px] text-zinc-400">{img.mimeType.replace('image/', '').toUpperCase()}</span>
            </div>
            <button
              type="button"
              onclick={() => removeAttachment(img.id)}
              aria-label={`Remove ${img.name}`}
              class="rounded p-0.5 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100 cursor-pointer"
            >
              <X class="h-3.5 w-3.5" />
            </button>
          </div>
        {/each}

        <!-- Code / Text / Other File Chips -->
        {#each fileAttachments as file (file.id)}
          <span
            class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700/80 bg-zinc-900/90 px-2.5 py-1 text-xs text-zinc-200"
            title={file.path}
          >
            <FileCode class="h-3.5 w-3.5 shrink-0 text-sky-400" />
            <span class="font-medium text-zinc-100">{file.name}</span>
            {#if file.relativePath && file.relativePath !== file.name}
              <span class="max-w-[160px] truncate font-mono text-[10px] text-zinc-400">
                {file.relativePath}
              </span>
            {:else if !file.insideWorkspace && file.path && file.path !== file.name}
              <span class="max-w-[160px] truncate font-mono text-[10px] text-amber-300/80">
                external
              </span>
            {/if}
            <button
              type="button"
              onclick={() => removeAttachment(file.id)}
              aria-label={`Remove ${file.name}`}
              class="ml-0.5 rounded p-0.5 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100 cursor-pointer"
            >
              <X class="h-3 w-3" />
            </button>
          </span>
        {/each}
      </div>
    {/if}

    <div class="flex flex-col gap-1.5 rounded-xl border border-zinc-800 bg-zinc-950/90 p-2 focus-within:border-indigo-500/50 transition">
      <div class="flex items-end gap-2">
        <textarea
          bind:this={textareaEl}
          bind:value={draft}
          oninput={handleInput}
          onkeydown={handleKeydown}
          onclick={detectSlashTrigger}
          disabled={!isConnected}
          rows="1"
          placeholder={!isConnected
            ? 'Start a Headless or Folder session above to begin chatting…'
            : chatStore.isAgentRunning
              ? 'Type to steer Pi mid-run, or type / for skills…'
              : 'Message Pi, type / for skills, or drag & drop files…'}
          class="max-h-44 min-h-[36px] flex-1 resize-none bg-transparent px-2 py-1.5 text-sm text-zinc-100 placeholder-zinc-400 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
        ></textarea>

        <div class="flex shrink-0 items-center gap-1.5">
          {#if !chatStore.isAgentRunning}
            <button
              type="button"
              onclick={() => void submitMessage()}
              disabled={!canSend}
              class="inline-flex items-center gap-1.5 rounded-lg bg-indigo-600 px-3 py-2 text-xs font-semibold text-white shadow transition hover:bg-indigo-500 disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
            >
              <Send class="h-3.5 w-3.5" />
              <span>Send</span>
            </button>
          {:else}
            <!-- Streaming state controls: Steer, Follow-up, Stop / Abort -->
            <button
              type="button"
              onclick={() => void submitMessage('steer')}
              disabled={!canSend}
              class="inline-flex items-center gap-1 rounded-lg border border-amber-500/40 bg-amber-500/15 px-2.5 py-2 text-xs font-medium text-amber-200 transition hover:bg-amber-500/25 disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
              title="Interrupt after current tool call and steer Pi immediately"
            >
              <Compass class="h-3.5 w-3.5" />
              <span>Steer</span>
            </button>

            <button
              type="button"
              onclick={() => void submitMessage('followUp')}
              disabled={!canSend}
              class="inline-flex items-center gap-1 rounded-lg border border-sky-500/40 bg-sky-500/15 px-2.5 py-2 text-xs font-medium text-sky-200 transition hover:bg-sky-500/25 disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
              title="Queue message to run after Pi finishes the current turn"
            >
              <Clock class="h-3.5 w-3.5" />
              <span class="hidden sm:inline">Follow-up</span>
            </button>

            <button
              type="button"
              onclick={() => chatStore.abort()}
              class="inline-flex items-center gap-1 rounded-lg border border-rose-500/40 bg-rose-600/20 px-2.5 py-2 text-xs font-semibold text-rose-200 transition hover:bg-rose-600/35 cursor-pointer"
              title="Clear queues and abort current generation (clear_queue + abort)"
            >
              <Square class="h-3.5 w-3.5 fill-current" />
              <span>Stop</span>
            </button>

            <button
              type="button"
              onclick={() => chatStore.stopSession()}
              class="inline-flex items-center gap-1 rounded-lg border border-zinc-700 bg-zinc-800/80 px-2 py-2 text-xs font-medium text-zinc-300 transition hover:border-rose-500/40 hover:bg-rose-500/15 hover:text-rose-200 cursor-pointer"
              title="Terminate Pi RPC child process (stop_pi_session)"
              aria-label="Stop Process"
            >
              <PowerOff class="h-3.5 w-3.5" />
              <span class="hidden md:inline">Stop Process</span>
            </button>
          {/if}
        </div>
      </div>

      <!-- Composer Bottom Toolbar: Model Selector, Skills Picker & Attach Files -->
      <div class="flex flex-wrap items-center justify-between gap-2 border-t border-zinc-800/70 pt-1.5 px-1">
        <div class="flex flex-wrap items-center gap-1.5">
          <ModelSelector placement="up" compact={true} />

          <div bind:this={skillToolbarWrapperEl} class="relative inline-flex">
            <button
              type="button"
              onclick={() => {
                isSkillPopoverOpen = !isSkillPopoverOpen;
                if (isSkillPopoverOpen) {
                  activeSlashMatch = null;
                }
              }}
              aria-haspopup="dialog"
              aria-expanded={isSkillPopoverOpen || activeSlashMatch !== null}
              class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-800 bg-zinc-900/80 px-2.5 py-1 text-xs text-zinc-200 transition hover:border-zinc-700 hover:bg-zinc-800/90 cursor-pointer"
              title="Browse installed skills, prompt templates, and slash commands (/)"
            >
              <Wand2 class="h-3.5 w-3.5 text-indigo-400" />
              <span>Skills</span>
              {#if chatStore.availableSkills.length > 0}
                <span class="rounded-full bg-zinc-800 px-1.5 py-0.2 font-mono text-[10px] text-zinc-300 tabular-nums">
                  {chatStore.availableSkills.length}
                </span>
              {/if}
            </button>

            <SkillPickerPopover
              bind:this={skillPopoverRef}
              open={isSkillPopoverOpen || activeSlashMatch !== null}
              slashQuery={activeSlashMatch?.query ?? null}
              {selectedSkillNames}
              onSelect={handleSelectSkill}
              onClose={() => {
                isSkillPopoverOpen = false;
                activeSlashMatch = null;
              }}
            />
          </div>

          <button
            type="button"
            onclick={() => void handlePickFiles()}
            disabled={isPickingFiles}
            class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-800 bg-zinc-900/80 px-2.5 py-1 text-xs text-zinc-200 transition hover:border-zinc-700 hover:bg-zinc-800/90 disabled:opacity-50 cursor-pointer"
            title="Attach images or source files to prompt"
          >
            <Paperclip class="h-3.5 w-3.5 text-sky-400" />
            <span>Attach Files</span>
          </button>
        </div>

        <span class="hidden sm:inline text-[11px] text-zinc-500">
          Type <kbd class="rounded border border-zinc-800 bg-zinc-900 px-1 py-0.2 font-mono text-[10px] text-zinc-300">/</kbd> for skills · Drop files to attach
        </span>
      </div>
    </div>
  </div>
</div>
