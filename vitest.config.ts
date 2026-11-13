import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

// Unit tests cover pure logic and runes stores (`*.svelte.ts`). Components are checked with
// `pnpm check`, so the SvelteKit and Tailwind plugins are not needed here.
export default defineConfig({
  plugins: [svelte()],
  test: {
    include: ['src/**/*.test.ts', 'scripts/**/*.test.ts']
  }
});
