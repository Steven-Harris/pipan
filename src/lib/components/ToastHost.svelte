<script lang="ts">
  // Transient notifications from extensions. Errors stay until dismissed.
  import { X } from 'lucide-svelte';
  import { notifications, type NotifyLevel } from '#lib/stores/notifications.svelte.ts';

  const tone: Record<NotifyLevel, string> = {
    info: 'border-zinc-700 bg-zinc-900 text-zinc-100',
    warning: 'border-amber-600/60 bg-amber-950/90 text-amber-100',
    error: 'border-red-600/60 bg-red-950/90 text-red-100'
  };
</script>

<div class="pointer-events-none fixed right-3 bottom-16 z-40 flex w-80 max-w-[calc(100vw-1.5rem)] flex-col gap-2">
  {#each notifications.toasts as toast (toast.id)}
    <div
      role={toast.level === 'error' ? 'alert' : 'status'}
      class="pointer-events-auto flex items-start gap-2 rounded-lg border px-3 py-2 text-xs shadow-lg {tone[toast.level]}"
    >
      <p class="min-w-0 flex-1 break-words whitespace-pre-wrap">{toast.message}</p>
      <button
        type="button"
        onclick={() => notifications.dismiss(toast.id)}
        class="shrink-0 rounded p-0.5 opacity-70 hover:opacity-100 cursor-pointer"
        aria-label="Dismiss notification"
      >
        <X class="h-3 w-3" />
      </button>
    </div>
  {/each}
</div>
