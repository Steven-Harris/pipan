<script lang="ts">
  import { Loader2, CheckCircle2, AlertCircle, CircleDot, Hourglass } from 'lucide-svelte';
  import type { FlowNodeStatus } from '#lib/types/pi.ts';

  let { status }: { status: FlowNodeStatus } = $props();
</script>

{#if status === 'running'}
  <Loader2 class="h-3.5 w-3.5 animate-spin text-indigo-300" aria-label="Running" />
{:else if status === 'error'}
  <AlertCircle class="h-3.5 w-3.5 text-rose-400" aria-label="Error" />
{:else if status === 'completed'}
  <CheckCircle2 class="h-3.5 w-3.5 text-emerald-400" aria-label="Completed" />
{:else if status === 'idle'}
  <CircleDot class="h-3.5 w-3.5 text-emerald-400" aria-label="Idle" />
{:else if status === 'waiting'}
  <Hourglass class="h-3.5 w-3.5 text-amber-300" aria-label="Waiting for approval" />
{:else}
  <span class="h-2 w-2 rounded-full bg-zinc-500" role="img" aria-label="Disconnected"></span>
{/if}
