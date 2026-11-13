import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

// Unit tests cover pure logic and runes stores (`*.svelte.ts`). Components are checked with
// `pnpm check`, so the SvelteKit and Tailwind plugins are not needed here.
//
// Modules are compiled for the client, so `$effect` runs in tests.
export default defineConfig({
  plugins: [svelte({ dynamicCompileOptions: () => ({ generate: 'client' }) })],
  resolve: {
    // `$effect` and `flushSync` are no-ops in Svelte's server entry, which Node resolves by default.
    alias: [
      {
        find: /^svelte$/,
        replacement: decodeURIComponent(new URL('./node_modules/svelte/src/index-client.js', import.meta.url).pathname)
      }
    ]
  },
  test: {
    include: ['src/**/*.test.ts', 'scripts/**/*.test.ts']
  }
});
