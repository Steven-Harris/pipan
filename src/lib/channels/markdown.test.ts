import { describe, expect, it } from 'vitest';
import { renderMarkdown, safeHref } from './markdown';

describe('renderMarkdown', () => {
  it('renders common markdown', () => {
    const html = renderMarkdown('# Title\n\nSome **bold** and `code`.\n\n- a\n- b');
    expect(html).toContain('<h1>Title</h1>');
    expect(html).toContain('<strong>bold</strong>');
    expect(html).toContain('<code>code</code>');
    expect(html).toContain('<li>a</li>');
  });

  it('shows raw HTML as text', () => {
    const html = renderMarkdown('<script>alert(1)</script>\n\nhello <img src=x onerror=alert(1)> there\n\n<div onclick="x()">d</div>');
    expect(html).not.toMatch(/<script|<img|<div/);
    expect(html).toContain('&lt;script&gt;');
  });

  it('drops unsafe link targets but keeps the text', () => {
    for (const href of ['javascript:alert(1)', 'data:text/html,x', 'vbscript:x', '/relative', '//host/x', 'JaVaScRiPt:alert(1)']) {
      const html = renderMarkdown(`[click](${href})`);
      expect(html).not.toContain('<a');
      expect(html).toContain('click');
    }
  });

  it('keeps safe links with a hardened rel and target', () => {
    const html = renderMarkdown('[docs](https://example.com/a?b=1&c=2) and <https://example.org>');
    expect(html).toContain('href="https://example.com/a?b=1&amp;c=2"');
    expect(html).toContain('rel="noopener noreferrer"');
    expect(html).toContain('href="https://example.org/"');
    expect(renderMarkdown('[m](mailto:a@example.com)')).toContain('mailto:a@example.com');
  });

  it('does not load images', () => {
    const html = renderMarkdown('![alt text](https://example.com/x.png)');
    expect(html).not.toContain('<img');
    expect(html).toContain('alt text');
  });

  it('escapes attribute breakouts in titles and code', () => {
    const html = renderMarkdown('[a](https://example.com "x\\" onmouseover=\\"y")\n\n```\n<b>&</b>\n```');
    expect(html).not.toMatch(/ onmouseover=/);
    expect(html).toContain('&lt;b&gt;&amp;&lt;/b&gt;');
  });
});

describe('safeHref', () => {
  it('allows http, https and mailto only', () => {
    expect(safeHref('https://a.test/x')).toBe('https://a.test/x');
    expect(safeHref('ftp://a.test')).toBeNull();
    expect(safeHref('not a url')).toBeNull();
    expect(safeHref(null)).toBeNull();
  });
});
