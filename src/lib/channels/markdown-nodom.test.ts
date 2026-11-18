import { describe, expect, it } from 'vitest';
import { renderMarkdown, renderMarkdownBlocks } from './markdown';

// Plain Node has no DOM, so DOMPurify cannot run. The text must come back escaped, never raw.
describe('without a DOM', () => {
  it('escapes the source instead of returning unsanitized HTML', () => {
    const html = renderMarkdown('<img src=x onerror=alert(1)> **bold**');
    expect(html).not.toContain('<img');
    expect(html).toContain('&lt;img');
  });

  it('escapes each block', () => {
    for (const block of renderMarkdownBlocks('# T\n\n<script>x</script>')) {
      expect(block.html).not.toContain('<script');
    }
  });
});
