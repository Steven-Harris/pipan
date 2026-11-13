<script lang="ts">
  // Producer-supplied markdown. renderMarkdown escapes raw HTML, drops images and restricts links.
  import { renderMarkdown } from '#lib/channels/markdown.ts';

  let { source }: { source: string } = $props();
  const html = $derived(renderMarkdown(source));
</script>

<div class="channel-md">{@html html}</div>

<style>
  .channel-md :global(> :last-child) {
    margin-bottom: 0;
  }
  .channel-md :global(p) {
    margin: 0 0 0.5rem;
  }
  .channel-md :global(h1),
  .channel-md :global(h2),
  .channel-md :global(h3) {
    margin: 0.5rem 0 0.25rem;
    font-weight: 600;
    color: rgb(244 244 245);
  }
  .channel-md :global(ul),
  .channel-md :global(ol) {
    margin: 0 0 0.5rem 1.1rem;
    list-style: revert;
  }
  .channel-md :global(a) {
    color: rgb(56 189 248);
    text-decoration: underline;
  }
  .channel-md :global(code) {
    background: rgb(39 39 42);
    border-radius: 0.25rem;
    padding: 0 0.25rem;
    font-size: 0.7rem;
  }
  .channel-md :global(pre) {
    overflow-x: auto;
    background: rgb(9 9 11);
    border-radius: 0.375rem;
    padding: 0.5rem;
    margin: 0 0 0.5rem;
  }
  .channel-md :global(pre code) {
    background: none;
    padding: 0;
  }
</style>
