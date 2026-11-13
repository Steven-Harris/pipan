import { describe, expect, it } from 'vitest';
import {
  MAX_GRAPH_EDGES,
  MAX_GRAPH_NODES,
  MAX_REVIEW_COMMENTS,
  MAX_REVIEW_DIFF_LINES,
  MAX_REVIEW_FILES
} from './limits';
import { validateForm, validateGraph, validateRequest, validateReview, validateState } from './validate';

const node = (id: string, extra: object = {}) => ({ id, label: id, status: 'pending', ...extra });

describe('validateGraph', () => {
  const base = { id: 'g', nodes: [node('a'), node('b')], edges: [{ from: 'a', to: 'b' }] };

  it('accepts a minimal graph and applies defaults', () => {
    const result = validateGraph(base);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.direction).toBe('TB');
      expect(result.value.nodes).toHaveLength(2);
      expect(result.notices).toEqual([]);
    }
  });

  it('ignores unknown keys and falls back on unknown enum values', () => {
    const result = validateGraph({
      ...base,
      surprise: true,
      direction: 'diagonal',
      nodes: [node('a', { status: 'exploding', tone: 'plaid', extra: 1 }), node('b')]
    });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value).not.toHaveProperty('surprise');
      expect(result.value.direction).toBe('TB');
      expect(result.value.nodes[0].status).toBe('pending');
      expect(result.value.nodes[0].tone).toBeUndefined();
      expect(result.value.nodes[0]).not.toHaveProperty('extra');
    }
  });

  it('keeps open kind strings', () => {
    const result = validateGraph({ ...base, nodes: [node('a', { kind: 'anything-goes' }), node('b')] });
    expect(result.ok && result.value.nodes[0].kind).toBe('anything-goes');
  });

  it('rejects missing ids, non-array nodes and duplicate node ids', () => {
    expect(validateGraph({ ...base, id: '' }).ok).toBe(false);
    expect(validateGraph({ ...base, nodes: 'x' }).ok).toBe(false);
    expect(validateGraph({ ...base, nodes: [node('a'), node('a')] }).ok).toBe(false);
    expect(validateGraph({ ...base, nodes: [{ id: 'a' }] }).ok).toBe(false);
    expect(validateGraph(null).ok).toBe(false);
  });

  it('drops edges that reference missing nodes and says so', () => {
    const result = validateGraph({ ...base, edges: [{ from: 'a', to: 'b' }, { from: 'a', to: 'zzz' }] });
    expect(result.ok && result.value.edges).toHaveLength(1);
    expect(result.ok && result.notices[0]).toMatch(/Dropped 1 edges/);
  });

  it('truncates over-limit nodes with a notice and removes their edges', () => {
    const nodes = Array.from({ length: MAX_GRAPH_NODES + 5 }, (_u, i) => node(`n${i}`));
    const edges = [
      { from: 'n0', to: 'n1' },
      { from: 'n0', to: `n${MAX_GRAPH_NODES + 1}` }
    ];
    const result = validateGraph({ id: 'g', nodes, edges });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.nodes).toHaveLength(MAX_GRAPH_NODES);
      expect(result.value.edges).toHaveLength(1);
      expect(result.notices.some((n) => n.includes(`first ${MAX_GRAPH_NODES}`))).toBe(true);
    }
  });

  it('truncates over-limit edges', () => {
    const edges = Array.from({ length: MAX_GRAPH_EDGES + 3 }, () => ({ from: 'a', to: 'b' }));
    const result = validateGraph({ ...base, edges });
    expect(result.ok && result.value.edges).toHaveLength(MAX_GRAPH_EDGES);
  });

  it('filters active ids to existing nodes and validates actions', () => {
    const result = validateGraph({
      ...base,
      active: ['a', 'ghost', 3],
      actions: [{ id: 'stop', label: 'Stop', style: 'danger', confirm: 'Sure?' }]
    });
    expect(result.ok && result.value.active).toEqual(['a']);
    expect(validateGraph({ ...base, actions: [{ label: 'no id' }] }).ok).toBe(false);
  });
});

describe('validateForm', () => {
  const base = {
    title: 'T',
    fields: [{ id: 'f', type: 'select', label: 'F', options: [{ value: 'a', label: 'A' }] }],
    actions: [{ id: 'go', label: 'Go' }]
  };

  it('keeps requiredFor on every field type and ignores a malformed one', () => {
    const result = validateForm({
      title: 'F',
      fields: [
        { id: 's', type: 'select', label: 'S', options: [{ value: 'a', label: 'A' }], required: true, requiredFor: ['go'] },
        { id: 'm', type: 'multiselect', label: 'M', options: [{ value: 'a', label: 'A' }], min: 1, requiredFor: ['go', 3] },
        { id: 't', type: 'text', label: 'T', required: true, requiredFor: 'go' }
      ],
      actions: [{ id: 'go', label: 'Go' }]
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.value.fields?.map((f) => f.requiredFor)).toEqual([['go'], ['go'], undefined]);
  });

  it('accepts all three field types', () => {
    const result = validateForm({
      ...base,
      fields: [
        base.fields[0],
        { id: 'm', type: 'multiselect', label: 'M', options: [{ value: 'x', label: 'X' }], default: ['x', 5], min: 1 },
        { id: 't', type: 'text', label: 'T', multiline: true, default: 'hi' }
      ]
    });
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.value.fields?.[1]).toMatchObject({ default: ['x'], min: 1 });
  });

  it('accepts a body with actions only', () => {
    expect(validateForm({ title: 'T', body: 'md', actions: [{ id: 'ok', label: 'OK' }] }).ok).toBe(true);
  });

  it('rejects no actions, unknown field types, duplicate ids and duplicate option values', () => {
    expect(validateForm({ ...base, actions: [] }).ok).toBe(false);
    expect(validateForm({ ...base, fields: [{ id: 'f', type: 'slider', label: 'F' }] }).ok).toBe(false);
    expect(validateForm({ ...base, fields: [base.fields[0], base.fields[0]] }).ok).toBe(false);
    expect(
      validateForm({
        ...base,
        fields: [{ ...base.fields[0], options: [{ value: 'a', label: 'A' }, { value: 'a', label: 'B' }] }]
      }).ok
    ).toBe(false);
    expect(validateForm({ actions: [{ id: 'a', label: 'A' }] }).ok).toBe(false);
  });
});

describe('validateReview', () => {
  const line = (kind: string, n: number) => ({ kind, text: `l${n}`, oldLine: n, newLine: n });
  const file = (path: string, lines = 2) => ({
    path,
    status: 'modified',
    hunks: [{ header: '@@', lines: Array.from({ length: lines }, (_u, i) => line('ctx', i + 1)) }]
  });
  const choices = [{ id: 'approve', label: 'Approve' }];
  const diff = { title: 'R', subject: { type: 'diff', files: [file('a.ts')] }, comments: [], choices };

  it('accepts a diff review', () => {
    const result = validateReview(diff);
    expect(result.ok).toBe(true);
  });

  it('accepts a document review and defaults the format', () => {
    const result = validateReview({
      title: 'R',
      subject: { type: 'document', documents: [{ path: 'a.md', text: '# hi' }] },
      comments: [],
      choices
    });
    expect(result.ok && result.value.subject.type === 'document' && result.value.subject.documents[0].format).toBe(
      'markdown'
    );
  });

  it('treats missing comments as empty and normalizes comment fields', () => {
    const { comments: _drop, ...noComments } = diff;
    expect(validateReview(noComments).ok).toBe(true);
    const result = validateReview({
      ...diff,
      comments: [{ id: 'c', path: 'a.ts', side: 'sideways', line: 2, endLine: 1, text: 't', status: 'weird' }]
    });
    expect(result.ok && result.value.comments[0]).toMatchObject({ side: 'new', status: 'open', endLine: undefined });
  });

  it('falls back to ctx for unknown line kinds', () => {
    const result = validateReview({
      ...diff,
      subject: { type: 'diff', files: [{ path: 'a', hunks: [{ header: '@@', lines: [{ kind: 'zzz', text: 'x' }] }] }] }
    });
    expect(result.ok && result.value.subject.type === 'diff' && result.value.subject.files[0].hunks[0].lines[0].kind).toBe(
      'ctx'
    );
  });

  it('rejects empty subjects, unknown subject types and empty or duplicate choices', () => {
    expect(validateReview({ ...diff, subject: { type: 'diff', files: [] } }).ok).toBe(false);
    expect(validateReview({ ...diff, subject: { type: 'document', documents: [] } }).ok).toBe(false);
    expect(validateReview({ ...diff, subject: { type: 'video' } }).ok).toBe(false);
    expect(validateReview({ ...diff, choices: [] }).ok).toBe(false);
    expect(validateReview({ ...diff, choices: [choices[0], choices[0]] }).ok).toBe(false);
  });

  it('rejects comments with a bad line number', () => {
    const comment = { id: 'c', path: 'a', line: 0, text: 't' };
    expect(validateReview({ ...diff, comments: [comment] }).ok).toBe(false);
    expect(validateReview({ ...diff, comments: [{ ...comment, line: 1.5 }] }).ok).toBe(false);
  });

  it('enforces the file, line and comment limits', () => {
    const files = Array.from({ length: MAX_REVIEW_FILES + 1 }, (_u, i) => file(`f${i}.ts`, 1));
    expect(validateReview({ ...diff, subject: { type: 'diff', files } }).ok).toBe(false);

    const atLimit = [file('big.ts', MAX_REVIEW_DIFF_LINES)];
    expect(validateReview({ ...diff, subject: { type: 'diff', files: atLimit } }).ok).toBe(true);
    const over = [file('big.ts', MAX_REVIEW_DIFF_LINES), file('one-more.ts', 1)];
    expect(validateReview({ ...diff, subject: { type: 'diff', files: over } }).ok).toBe(false);

    const comments = Array.from({ length: MAX_REVIEW_COMMENTS + 1 }, (_u, i) => ({
      id: `c${i}`,
      path: 'a.ts',
      line: 1,
      text: 't'
    }));
    expect(validateReview({ ...diff, comments }).ok).toBe(false);
  });
});

describe('dispatch', () => {
  it('validates known primitives and passes unknown ones through', () => {
    expect(validateState('graph', {}).ok).toBe(false);
    expect(validateState('custom-thing', { any: 1 })).toEqual({ ok: true, value: { any: 1 }, notices: [] });
    expect(validateRequest('form', {}).ok).toBe(false);
    expect(validateRequest('review', {}).ok).toBe(false);
    expect(validateRequest('mystery', 5)).toEqual({ ok: true, value: 5, notices: [] });
  });
});

describe('unique keys', () => {
  const line = { kind: 'ctx', text: 'x', oldLine: 1, newLine: 1 };
  const file = (path: string) => ({ path, status: 'modified', hunks: [{ header: '@@', lines: [line] }] });
  const choices = [{ id: 'approve', label: 'Approve' }];
  const comment = (id: string) => ({
    id,
    path: 'a.ts',
    line: 1,
    side: 'new',
    text: 'note',
    author: 'user',
    status: 'open'
  });

  it('rejects two diff files with the same path', () => {
    const result = validateReview({
      title: 'R',
      subject: { type: 'diff', files: [file('a.ts'), file('a.ts')] },
      choices
    });
    expect(result).toMatchObject({ ok: false });
    expect(!result.ok && result.error).toContain('duplicate path "a.ts"');
  });

  it('rejects two documents with the same path', () => {
    const result = validateReview({
      title: 'R',
      subject: {
        type: 'document',
        documents: [
          { path: 'a.md', text: 'a' },
          { path: 'a.md', text: 'b' }
        ]
      },
      choices
    });
    expect(result.ok).toBe(false);
  });

  it('rejects two comments with the same id', () => {
    const result = validateReview({
      title: 'R',
      subject: { type: 'diff', files: [file('a.ts')] },
      comments: [comment('c1'), comment('c1')],
      choices
    });
    expect(result.ok).toBe(false);
  });

  it('drops a repeated graph action id and says so, keeping the first', () => {
    const result = validateGraph({
      id: 'g',
      nodes: [
        node('a', {
          actions: [
            { id: 'go', label: 'First' },
            { id: 'go', label: 'Second' },
            { id: 'stop', label: 'Stop' }
          ]
        })
      ],
      edges: [],
      actions: [
        { id: 'pause', label: 'Pause' },
        { id: 'pause', label: 'Pause again' }
      ]
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.value.nodes[0].actions?.map((a) => a.label)).toEqual(['First', 'Stop']);
    expect(result.value.actions?.map((a) => a.label)).toEqual(['Pause']);
    expect(result.notices).toHaveLength(2);
    expect(result.notices.join(' ')).toContain('node "a"');
    expect(result.notices.join(' ')).toContain('the graph');
  });

  it('accepts an idle graph with no nodes and a start action', () => {
    const result = validateGraph({
      id: 'idle',
      nodes: [],
      edges: [],
      actions: [{ id: 'start', label: 'Start' }]
    });
    expect(result.ok && result.value.actions?.length).toBe(1);
  });
});
