/**
 * Hand-written validators for the channel primitives. They return a normalized copy: unknown
 * keys are dropped, unknown enum values fall back to the documented default, and anything the
 * spec requires but is missing or mistyped fails the whole payload. Limit violations follow
 * spec section 3 (graph truncates, review rejects).
 */

import {
  MAX_GRAPH_EDGES,
  MAX_GRAPH_NODES,
  MAX_REVIEW_COMMENTS,
  MAX_REVIEW_DIFF_LINES,
  MAX_REVIEW_FILES
} from './limits';
import type {
  ActionStyle,
  CommentStatus,
  DiffFile,
  DiffHunk,
  DiffLine,
  FormAction,
  FormField,
  FormOption,
  FormRequest,
  GraphAction,
  GraphEdge,
  GraphNode,
  GraphState,
  NodeDetail,
  NodeStatus,
  ReviewChoice,
  ReviewComment,
  ReviewDocument,
  ReviewRequest,
  Tone
} from './types';

export type Validated<T> =
  | { ok: true; value: T; notices: string[] }
  | { ok: false; error: string };

class Invalid extends Error {}

const bad = (message: string): never => {
  throw new Invalid(message);
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

function run<T>(build: (notices: string[]) => T): Validated<T> {
  const notices: string[] = [];
  try {
    return { ok: true, value: build(notices), notices };
  } catch (error) {
    if (error instanceof Invalid) return { ok: false, error: error.message };
    throw error;
  }
}

// ---- field helpers -------------------------------------------------------------------------

function rec(value: unknown, path: string): Record<string, unknown> {
  return isRecord(value) ? value : bad(`${path} must be an object`);
}

function str(value: unknown, path: string): string {
  return typeof value === 'string' ? value : bad(`${path} must be a string`);
}

function nonEmpty(value: unknown, path: string): string {
  const text = str(value, path);
  return text === '' ? bad(`${path} must not be empty`) : text;
}

const optStr = (value: unknown): string | undefined => (typeof value === 'string' ? value : undefined);
const optNum = (value: unknown): number | undefined =>
  typeof value === 'number' && Number.isFinite(value) ? value : undefined;
const optBool = (value: unknown): boolean | undefined => (typeof value === 'boolean' ? value : undefined);

function lineNumber(value: unknown, path: string): number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 1
    ? value
    : bad(`${path} must be a positive integer`);
}

function arr(value: unknown, path: string): unknown[] {
  return Array.isArray(value) ? value : bad(`${path} must be an array`);
}

function oneOf<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return typeof value === 'string' && (allowed as readonly string[]).includes(value) ? (value as T) : fallback;
}

function optOneOf<T extends string>(value: unknown, allowed: readonly T[]): T | undefined {
  return typeof value === 'string' && (allowed as readonly string[]).includes(value) ? (value as T) : undefined;
}

const STYLES = ['default', 'primary', 'danger'] as const satisfies readonly ActionStyle[];
const TONES = ['neutral', 'cyan', 'amber', 'violet', 'green', 'red', 'blue'] as const satisfies readonly Tone[];
const NODE_STATUSES = [
  'pending',
  'active',
  'waiting',
  'done',
  'failed',
  'skipped'
] as const satisfies readonly NodeStatus[];
const COMMENT_STATUSES = [
  'open',
  'addressed',
  'declined',
  'dismissed'
] as const satisfies readonly CommentStatus[];

function uniqueIds(items: { id: string }[], path: string): void {
  const seen = new Set<string>();
  for (const item of items) {
    if (seen.has(item.id)) bad(`${path} has a duplicate id "${item.id}"`);
    seen.add(item.id);
  }
}

function uniqueBy<T>(items: T[], key: (item: T) => string, path: string, what: string): void {
  const seen = new Set<string>();
  for (const item of items) {
    const value = key(item);
    if (seen.has(value)) bad(`${path} has a duplicate ${what} "${value}"`);
    seen.add(value);
  }
}

// ---- graph ---------------------------------------------------------------------------------

function action(value: unknown, path: string): GraphAction {
  const raw = rec(value, path);
  return {
    id: nonEmpty(raw.id, `${path}.id`),
    label: str(raw.label, `${path}.label`),
    enabled: optBool(raw.enabled),
    confirm: optStr(raw.confirm),
    style: optOneOf(raw.style, STYLES)
  };
}

/** Graph actions are buttons. A later action that repeats an id is dropped, not an error. */
function actions(value: unknown, path: string, notices: string[], where: string): GraphAction[] | undefined {
  if (value === undefined) return undefined;
  const seen = new Set<string>();
  const kept: GraphAction[] = [];
  for (const [i, a] of arr(value, path).entries()) {
    const item = action(a, `${path}[${i}]`);
    if (seen.has(item.id)) {
      notices.push(`Dropped a duplicate action "${item.id}" on ${where}.`);
      continue;
    }
    seen.add(item.id);
    kept.push(item);
  }
  return kept;
}

function detail(value: unknown): NodeDetail | undefined {
  if (!isRecord(value)) return undefined;
  const fields = Array.isArray(value.fields)
    ? value.fields.flatMap((f) =>
        isRecord(f) && typeof f.label === 'string' && typeof f.value === 'string'
          ? [{ label: f.label, value: f.value }]
          : []
      )
    : undefined;
  return {
    summary: optStr(value.summary),
    markdown: optStr(value.markdown),
    fields,
    startedAt: optNum(value.startedAt),
    endedAt: optNum(value.endedAt),
    durationMs: optNum(value.durationMs),
    costUsd: optNum(value.costUsd),
    tokens: optNum(value.tokens)
  };
}

function graphNode(value: unknown, path: string, notices: string[]): GraphNode {
  const raw = rec(value, path);
  const id = nonEmpty(raw.id, `${path}.id`);
  return {
    id,
    label: str(raw.label, `${path}.label`),
    kind: optStr(raw.kind),
    tone: optOneOf(raw.tone, TONES),
    status: oneOf(raw.status, NODE_STATUSES, 'pending'),
    visits: optNum(raw.visits),
    group: optStr(raw.group),
    detail: detail(raw.detail),
    actions: actions(raw.actions, `${path}.actions`, notices, `node "${id}"`)
  };
}

export function validateGraph(data: unknown): Validated<GraphState> {
  return run((notices) => {
    const raw = rec(data, 'graph');
    const id = nonEmpty(raw.id, 'graph.id');
    let nodes = arr(raw.nodes, 'graph.nodes').map((n, i) => graphNode(n, `graph.nodes[${i}]`, notices));
    uniqueIds(nodes, 'graph.nodes');
    if (nodes.length > MAX_GRAPH_NODES) {
      notices.push(`Showing the first ${MAX_GRAPH_NODES} of ${nodes.length} nodes.`);
      nodes = nodes.slice(0, MAX_GRAPH_NODES);
    }
    const nodeIds = new Set(nodes.map((n) => n.id));

    let edges: GraphEdge[] = arr(raw.edges, 'graph.edges').map((e, i) => {
      const edge = rec(e, `graph.edges[${i}]`);
      return {
        id: optStr(edge.id),
        from: nonEmpty(edge.from, `graph.edges[${i}].from`),
        to: nonEmpty(edge.to, `graph.edges[${i}].to`),
        label: optStr(edge.label),
        taken: optNum(edge.taken)
      };
    });
    const known = edges.filter((e) => nodeIds.has(e.from) && nodeIds.has(e.to));
    if (known.length < edges.length) {
      notices.push(`Dropped ${edges.length - known.length} edges that reference missing nodes.`);
    }
    edges = known;
    if (edges.length > MAX_GRAPH_EDGES) {
      notices.push(`Showing the first ${MAX_GRAPH_EDGES} of ${edges.length} edges.`);
      edges = edges.slice(0, MAX_GRAPH_EDGES);
    }

    return {
      id,
      title: optStr(raw.title),
      subtitle: optStr(raw.subtitle),
      direction: oneOf(raw.direction, ['TB', 'LR'] as const, 'TB'),
      nodes,
      edges,
      active: Array.isArray(raw.active)
        ? raw.active.filter((a): a is string => typeof a === 'string' && nodeIds.has(a))
        : undefined,
      actions: actions(raw.actions, 'graph.actions', notices, 'the graph'),
      note: optStr(raw.note)
    };
  });
}

// ---- form ----------------------------------------------------------------------------------

function options(value: unknown, path: string): FormOption[] {
  const list = arr(value, path).map((o, i) => {
    const raw = rec(o, `${path}[${i}]`);
    return {
      value: str(raw.value, `${path}[${i}].value`),
      label: str(raw.label, `${path}[${i}].label`),
      description: optStr(raw.description)
    };
  });
  const seen = new Set<string>();
  for (const option of list) {
    if (seen.has(option.value)) bad(`${path} has a duplicate value "${option.value}"`);
    seen.add(option.value);
  }
  return list;
}

/** Action ids a requirement applies to. Not-a-list values are ignored, as are non-string entries. */
function requiredFor(value: unknown): string[] | undefined {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : undefined;
}

/** Ids of alternative fields. Not-a-list values are ignored, as are non-string entries. */
const exclusiveWith = (value: unknown): string[] | undefined => requiredFor(value);

function formField(value: unknown, path: string): FormField {
  const raw = rec(value, path);
  const id = nonEmpty(raw.id, `${path}.id`);
  const label = str(raw.label, `${path}.label`);
  switch (raw.type) {
    case 'select':
      return {
        id,
        type: 'select',
        label,
        options: options(raw.options, `${path}.options`),
        default: optStr(raw.default),
        required: optBool(raw.required),
        requiredFor: requiredFor(raw.requiredFor),
        exclusiveWith: exclusiveWith(raw.exclusiveWith)
      };
    case 'multiselect':
      return {
        id,
        type: 'multiselect',
        label,
        options: options(raw.options, `${path}.options`),
        default: Array.isArray(raw.default)
          ? raw.default.filter((d): d is string => typeof d === 'string')
          : undefined,
        min: optNum(raw.min),
        max: optNum(raw.max),
        requiredFor: requiredFor(raw.requiredFor)
      };
    case 'text':
      return {
        id,
        type: 'text',
        label,
        multiline: optBool(raw.multiline),
        placeholder: optStr(raw.placeholder),
        default: optStr(raw.default),
        required: optBool(raw.required),
        requiredFor: requiredFor(raw.requiredFor),
        exclusiveWith: exclusiveWith(raw.exclusiveWith)
      };
    default:
      return bad(`${path}.type "${String(raw.type)}" is not a known field type`);
  }
}

export function validateForm(data: unknown): Validated<FormRequest> {
  return run(() => {
    const raw = rec(data, 'form');
    const fields = raw.fields === undefined ? undefined : arr(raw.fields, 'form.fields').map((f, i) => formField(f, `form.fields[${i}]`));
    if (fields) uniqueIds(fields, 'form.fields');
    const formActions: FormAction[] = arr(raw.actions, 'form.actions').map((a, i) => {
      const item = rec(a, `form.actions[${i}]`);
      return {
        id: nonEmpty(item.id, `form.actions[${i}].id`),
        label: str(item.label, `form.actions[${i}].label`),
        description: optStr(item.description),
        style: optOneOf(item.style, STYLES),
        skipValidation: optBool(item.skipValidation)
      };
    });
    if (formActions.length === 0) bad('form.actions must have at least one action');
    uniqueIds(formActions, 'form.actions');
    return { title: str(raw.title, 'form.title'), body: optStr(raw.body), fields, actions: formActions };
  });
}

// ---- review --------------------------------------------------------------------------------

function diffLine(value: unknown, path: string): DiffLine {
  const raw = rec(value, path);
  return {
    kind: oneOf(raw.kind, ['add', 'del', 'ctx'] as const, 'ctx'),
    text: str(raw.text, `${path}.text`),
    oldLine: optNum(raw.oldLine),
    newLine: optNum(raw.newLine)
  };
}

function diffHunk(value: unknown, path: string, counter: { lines: number }): DiffHunk {
  const raw = rec(value, path);
  const lines = arr(raw.lines, `${path}.lines`);
  counter.lines += lines.length;
  if (counter.lines > MAX_REVIEW_DIFF_LINES) {
    bad(`review has more than ${MAX_REVIEW_DIFF_LINES} diff lines`);
  }
  return {
    header: str(raw.header, `${path}.header`),
    lines: lines.map((l, i) => diffLine(l, `${path}.lines[${i}]`))
  };
}

function diffFile(value: unknown, path: string, counter: { lines: number }): DiffFile {
  const raw = rec(value, path);
  return {
    path: nonEmpty(raw.path, `${path}.path`),
    oldPath: optStr(raw.oldPath),
    status: oneOf(raw.status, ['added', 'modified', 'deleted', 'renamed', 'untracked'] as const, 'modified'),
    binary: optBool(raw.binary),
    truncated: optBool(raw.truncated),
    hunks: arr(raw.hunks ?? [], `${path}.hunks`).map((h, i) => diffHunk(h, `${path}.hunks[${i}]`, counter))
  };
}

function reviewComment(value: unknown, path: string): ReviewComment {
  const raw = rec(value, path);
  const line = lineNumber(raw.line, `${path}.line`);
  const endLine = optNum(raw.endLine);
  const replies = Array.isArray(raw.replies)
    ? raw.replies.flatMap((r) =>
        isRecord(r) && typeof r.text === 'string'
          ? [{ author: oneOf(r.author, ['user', 'agent'] as const, 'agent'), text: r.text }]
          : []
      )
    : undefined;
  return {
    id: nonEmpty(raw.id, `${path}.id`),
    path: nonEmpty(raw.path, `${path}.path`),
    side: oneOf(raw.side, ['old', 'new'] as const, 'new'),
    line,
    endLine: endLine !== undefined && endLine >= line ? endLine : undefined,
    snippet: optStr(raw.snippet),
    text: str(raw.text, `${path}.text`),
    status: oneOf(raw.status, COMMENT_STATUSES, 'open'),
    outdated: optBool(raw.outdated),
    replies
  };
}

function reviewChoice(value: unknown, path: string): ReviewChoice {
  const raw = rec(value, path);
  return {
    id: nonEmpty(raw.id, `${path}.id`),
    label: str(raw.label, `${path}.label`),
    description: optStr(raw.description),
    style: optOneOf(raw.style, STYLES),
    requiresComment: optBool(raw.requiresComment)
  };
}

export function validateReview(data: unknown): Validated<ReviewRequest> {
  return run(() => {
    const raw = rec(data, 'review');
    const subjectRaw = rec(raw.subject, 'review.subject');
    let subject: ReviewRequest['subject'];

    if (subjectRaw.type === 'diff') {
      const files = arr(subjectRaw.files, 'review.subject.files');
      if (files.length === 0) bad('review.subject.files must not be empty');
      if (files.length > MAX_REVIEW_FILES) bad(`review has more than ${MAX_REVIEW_FILES} files`);
      const counter = { lines: 0 };
      const diffFiles = files.map((f, i) => diffFile(f, `review.subject.files[${i}]`, counter));
      uniqueBy(diffFiles, (f) => f.path, 'review.subject.files', 'path');
      subject = { type: 'diff', label: optStr(subjectRaw.label), files: diffFiles };
    } else if (subjectRaw.type === 'document') {
      const documents = arr(subjectRaw.documents, 'review.subject.documents');
      if (documents.length === 0) bad('review.subject.documents must not be empty');
      if (documents.length > MAX_REVIEW_FILES) bad(`review has more than ${MAX_REVIEW_FILES} documents`);
      const docs = documents.map((d, i): ReviewDocument => {
          const doc = rec(d, `review.subject.documents[${i}]`);
          return {
            path: nonEmpty(doc.path, `review.subject.documents[${i}].path`),
            text: str(doc.text, `review.subject.documents[${i}].text`),
            format: oneOf(doc.format, ['markdown', 'text'] as const, 'markdown')
          };
        });
      uniqueBy(docs, (d) => d.path, 'review.subject.documents', 'path');
      subject = { type: 'document', documents: docs };
    } else {
      return bad(`review.subject.type "${String(subjectRaw.type)}" is not a known subject`);
    }

    const comments = arr(raw.comments ?? [], 'review.comments');
    if (comments.length > MAX_REVIEW_COMMENTS) bad(`review has more than ${MAX_REVIEW_COMMENTS} comments`);
    const choices = arr(raw.choices, 'review.choices').map((c, i) => reviewChoice(c, `review.choices[${i}]`));
    if (choices.length === 0) bad('review.choices must have at least one choice');
    uniqueIds(choices, 'review.choices');
    const reviewComments = comments.map((c, i) => reviewComment(c, `review.comments[${i}]`));
    uniqueIds(reviewComments, 'review.comments');

    return {
      title: str(raw.title, 'review.title'),
      body: optStr(raw.body),
      subject,
      comments: reviewComments,
      choices,
      commentHint: optStr(raw.commentHint)
    };
  });
}

// ---- dispatch ------------------------------------------------------------------------------

/** Primitives the host validates. Others pass through for tier 2 plugins. */
export const KNOWN_STATE_PRIMITIVES = ['graph'] as const;
export const KNOWN_REQUEST_KINDS = ['form', 'review'] as const;

export function validateState(primitive: string, data: unknown): Validated<unknown> {
  if (primitive === 'graph') return validateGraph(data);
  return { ok: true, value: data, notices: [] };
}

export function validateRequest(kind: string, payload: unknown): Validated<unknown> {
  if (kind === 'form') return validateForm(payload);
  if (kind === 'review') return validateReview(payload);
  return { ok: true, value: payload, notices: [] };
}
