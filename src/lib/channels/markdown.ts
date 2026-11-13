/**
 * Markdown for producer-supplied text. Producers are untrusted, so output is restricted twice.
 * The renderer shows raw HTML as text, replaces images with their alt text (nothing loads
 * remotely), and keeps only http, https and mailto link targets. DOMPurify then sanitizes the
 * rendered HTML against a strict allowlist, and its result is the only value that reaches
 * `{@html}`. Without a DOM (plain Node), the text is returned escaped.
 */
import DOMPurify from 'dompurify';
import { Marked, type Token, type Tokens } from 'marked';

const escapeHtml = (text: string): string =>
  text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');

const SAFE_PROTOCOLS = new Set(['http:', 'https:', 'mailto:']);

/** Returns the href when it is an absolute http, https or mailto URL, otherwise null. */
export function safeHref(href: string | null | undefined): string | null {
  if (!href) return null;
  try {
    const url = new URL(href.trim());
    return SAFE_PROTOCOLS.has(url.protocol) ? url.href : null;
  } catch {
    return null;
  }
}

const marked = new Marked({
  gfm: true,
  async: false,
  renderer: {
    html({ text }) {
      return escapeHtml(text);
    },
    image({ text }) {
      return escapeHtml(text);
    },
    link({ href, tokens }) {
      const inner = this.parser.parseInline(tokens);
      const safe = safeHref(href);
      return safe ? `<a href="${escapeHtml(safe)}" rel="noopener noreferrer" target="_blank">${inner}</a>` : inner;
    }
  }
});

const ALLOWED_TAGS = [
  'a',
  'p',
  'br',
  'hr',
  'strong',
  'em',
  'del',
  'code',
  'pre',
  'blockquote',
  'ul',
  'ol',
  'li',
  'h1',
  'h2',
  'h3',
  'h4',
  'h5',
  'h6',
  'table',
  'thead',
  'tbody',
  'tr',
  'th',
  'td',
  'input'
];
const ALLOWED_ATTR = ['href', 'rel', 'target', 'align', 'start', 'type', 'checked', 'disabled'];

let hooked = false;

/** Sanitizes rendered HTML. Returns null when no DOM is available. */
export function sanitizeHtml(html: string): string | null {
  if (!DOMPurify.isSupported) return null;
  if (!hooked) {
    hooked = true;
    DOMPurify.addHook('afterSanitizeAttributes', (node) => {
      if (node.tagName === 'A') {
        node.setAttribute('rel', 'noopener noreferrer');
        node.setAttribute('target', '_blank');
      }
      // The only input marked emits is a disabled task-list checkbox.
      if (node.tagName === 'INPUT') {
        if (node.getAttribute('type') !== 'checkbox') node.remove();
        else node.setAttribute('disabled', '');
      }
    });
  }
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
    // Only href is a URL. The other allowed attributes carry plain words or numbers.
    ADD_URI_SAFE_ATTR: ['rel', 'target', 'align', 'start', 'type', 'checked', 'disabled'],
    ALLOWED_URI_REGEXP: /^(?:https?|mailto):/i
  });
}

/** Sanitizes rendered HTML, or escapes the original text when there is no DOM. */
const clean = (html: string, source: string): string => sanitizeHtml(html) ?? `<p>${escapeHtml(source)}</p>`;

/** Renders markdown to an HTML string that is safe to use with `{@html}`. */
export function renderMarkdown(source: string): string {
  return clean(marked.parse(source) as string, source);
}

/** A top-level piece of a document: its rendered HTML and the source lines it came from. */
export interface MarkdownBlock {
  /** 1-based first source line. */
  startLine: number;
  /** 1-based last source line, inclusive. */
  endLine: number;
  /** HTML that is safe to use with `{@html}`. */
  html: string;
}

const countNewlines = (text: string): number => {
  let n = 0;
  for (let i = text.indexOf('\n'); i >= 0; i = text.indexOf('\n', i + 1)) n++;
  return n;
};

/** Source lines a token covers, ignoring the newlines that end it. */
const spanOf = (raw: string): number => countNewlines(raw.replace(/\n+$/, ''));

/**
 * Splits markdown into blocks that each know their source lines. Lists are split per item so a
 * comment can point at one item. Line numbers follow the text after line endings are normalized,
 * which keeps them equal to the numbers an editor shows.
 */
export function renderMarkdownBlocks(source: string): MarkdownBlock[] {
  const text = source.replace(/\r\n|\r/g, '\n');
  const tokens = marked.lexer(text);
  const links = tokens.links;
  const render = (list: Token[]): string => marked.parser(Object.assign(list, { links })) as string;

  const blocks: MarkdownBlock[] = [];
  let line = 1;
  for (const token of tokens) {
    const start = line;
    line += countNewlines(token.raw);
    if (token.type === 'space' || token.type === 'def') continue;

    if (token.type === 'list') {
      const list = token as Tokens.List;
      let cursor = 0;
      list.items.forEach((item, index) => {
        const at = list.raw.indexOf(item.raw, cursor);
        const offset = at < 0 ? cursor : at;
        cursor = offset + item.raw.length;
        const itemStart = start + countNewlines(list.raw.slice(0, offset));
        const single: Tokens.List = {
          ...list,
          raw: item.raw,
          items: [item],
          start: list.ordered && typeof list.start === 'number' ? list.start + index : list.start
        };
        blocks.push({ startLine: itemStart, endLine: itemStart + spanOf(item.raw), html: clean(render([single]), item.raw) });
      });
      continue;
    }

    blocks.push({ startLine: start, endLine: start + spanOf(token.raw), html: clean(render([token]), token.raw) });
  }
  return blocks;
}
