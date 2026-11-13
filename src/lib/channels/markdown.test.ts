// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { renderMarkdown, renderMarkdownBlocks, safeHref, sanitizeHtml } from './markdown';

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

describe('second layer: DOMPurify over rendered HTML', () => {
  const run = (html: string): string => sanitizeHtml(html) ?? '';

  it('removes dangerous elements and attributes that the renderer would never emit', () => {
    const cases = [
      '<script>alert(1)</script><p>x</p>',
      '<p onclick="x()">x</p>',
      '<iframe src="https://example.com"></iframe>',
      '<form action="/x"><button>go</button></form>',
      '<style>p{background:url(x)}</style><p>x</p>',
      '<img src=x onerror=alert(1)>',
      '<svg><script>alert(1)</script></svg>',
      '<math><mi><mglyph><style><img src=x onerror=alert(1)></style></mglyph></mi></math>',
      '<p data-x="1" style="color:red" id="a" class="b">x</p>',
      '<object data="x"></object><embed src="x">',
      '<a href="javascript:alert(1)">x</a>',
      '<a href="data:text/html,x">x</a>',
      '<a href=" javascript:alert(1)">x</a>',
      '<noscript><p title="</noscript><img src=x onerror=alert(1)>"></noscript>',
      '<table><td><svg></p><style><a id="</style><img src=1 onerror=alert(1)>">'
    ];
    for (const html of cases) {
      const out = run(html);
      expect(out, html).not.toMatch(/<script|<iframe|<form|<style|<img|<svg|<math|<object|<embed|<noscript/i);
      expect(out, html).not.toMatch(/\son\w+=|javascript:|data:|\sstyle=|\sdata-|\sid=|\sclass=/i);
    }
  });

  it('forces rel and target on links and keeps only allowed schemes', () => {
    const out = run('<a href="https://example.com" target="_self" rel="opener">x</a><a href="ftp://example.com">y</a>');
    expect(out).toContain('rel="noopener noreferrer"');
    expect(out).toContain('target="_blank"');
    expect(out).not.toContain('ftp:');
  });

  it('keeps only disabled checkboxes as inputs', () => {
    const out = run('<input type="text" value="x"><input type="checkbox" checked>');
    expect(out).not.toContain('type="text"');
    expect(out).toMatch(/<input[^>]*type="checkbox"[^>]*disabled/);
  });
});

describe('rendered markdown', () => {
  it('keeps structure and shows hostile text inside tables and code as text', () => {
    const html = renderMarkdown(
      '| a | b |\n|---|---|\n| <img src=x onerror=alert(1)> | `<script>x</script>` |\n\n```html\n<iframe src=x></iframe>\n```\n\n- [x] done'
    );
    expect(html).toContain('<table>');
    expect(html).toContain('<pre>');
    expect(html).not.toMatch(/<img|<script|<iframe/);
    const input = html.match(/<input[^>]*>/)?.[0] ?? '';
    expect(input).toContain('type="checkbox"');
    expect(input).toContain('disabled');
  });

  it('sanitizes every block of a document', () => {
    const blocks = renderMarkdownBlocks('# T\n\n<script>alert(1)</script>\n\n- a\n- [l](javascript:x)\n');
    expect(blocks.length).toBeGreaterThanOrEqual(3);
    for (const block of blocks) expect(block.html).not.toMatch(/<script|javascript:/);
  });
});
