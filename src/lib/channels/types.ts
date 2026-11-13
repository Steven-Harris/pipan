/**
 * Extension channel types. Canonical source for docs/plans/extension-channel-spec.md.
 * Wire-level types are in sections 2 and 3, primitives in sections 4 to 6.
 */

export const PROTOCOL_VERSION = 1;

// ---- Wire ----------------------------------------------------------------------------------

export interface HostInfo {
  protocol: 1;
  host: 'pipan';
  primitives: Record<string, number>;
}

export interface Envelope<T> {
  v: 1;
  ch: string;
  seq?: number;
  payload: T;
}

export interface StatePayload {
  primitive: string;
  data: unknown;
}

export interface RequestPayload {
  requestId: string;
  payload: unknown;
}

export interface IntentPayload {
  v: 1;
  ch: string;
  name: string;
  payload: unknown;
}

// ---- graph ---------------------------------------------------------------------------------

export type Tone = 'neutral' | 'cyan' | 'amber' | 'violet' | 'green' | 'red' | 'blue';
export type NodeStatus = 'pending' | 'active' | 'waiting' | 'done' | 'failed' | 'skipped';
export type ActionStyle = 'default' | 'primary' | 'danger';

export interface GraphAction {
  id: string;
  label: string;
  enabled?: boolean;
  confirm?: string;
  style?: ActionStyle;
}

export interface NodeDetail {
  summary?: string;
  markdown?: string;
  fields?: { label: string; value: string }[];
  startedAt?: number;
  endedAt?: number;
  durationMs?: number;
  costUsd?: number;
  tokens?: number;
}

export interface GraphNode {
  id: string;
  label: string;
  kind?: string;
  tone?: Tone;
  status: NodeStatus;
  visits?: number;
  group?: string;
  detail?: NodeDetail;
  actions?: GraphAction[];
}

export interface GraphEdge {
  id?: string;
  from: string;
  to: string;
  label?: string;
  taken?: number;
}

export interface GraphState {
  id: string;
  title?: string;
  subtitle?: string;
  direction?: 'TB' | 'LR';
  nodes: GraphNode[];
  edges: GraphEdge[];
  active?: string[];
  actions?: GraphAction[];
  note?: string;
}

// ---- review --------------------------------------------------------------------------------

export interface DiffLine {
  kind: 'add' | 'del' | 'ctx';
  text: string;
  oldLine?: number;
  newLine?: number;
}

export interface DiffHunk {
  header: string;
  lines: DiffLine[];
}

export interface DiffFile {
  path: string;
  oldPath?: string;
  status: 'added' | 'modified' | 'deleted' | 'renamed' | 'untracked';
  binary?: boolean;
  truncated?: boolean;
  hunks: DiffHunk[];
}

export interface DiffSubject {
  type: 'diff';
  files: DiffFile[];
  label?: string;
}

export interface ReviewDocument {
  path: string;
  text: string;
  format?: 'markdown' | 'text';
}

export interface DocumentSubject {
  type: 'document';
  documents: ReviewDocument[];
}

export type CommentStatus = 'open' | 'addressed' | 'declined' | 'dismissed';

export interface ReviewComment {
  id: string;
  path: string;
  side: 'old' | 'new';
  line: number;
  endLine?: number;
  snippet?: string;
  text: string;
  status: CommentStatus;
  outdated?: boolean;
  replies?: { author: 'user' | 'agent'; text: string }[];
}

export interface ReviewChoice {
  id: string;
  label: string;
  description?: string;
  style?: ActionStyle;
  requiresComment?: boolean;
}

export interface ReviewRequest {
  title: string;
  body?: string;
  subject: DiffSubject | DocumentSubject;
  comments: ReviewComment[];
  choices: ReviewChoice[];
  commentHint?: string;
}

export interface ReviewAnswer {
  choice: string;
  comments: ReviewComment[];
  touched: string[];
  note?: string;
}

// ---- form ----------------------------------------------------------------------------------

export interface FormOption {
  value: string;
  label: string;
  description?: string;
}

export type FormField =
  | {
      id: string;
      type: 'select';
      label: string;
      options: FormOption[];
      default?: string;
      required?: boolean;
      /** Action ids that `required` applies to. Absent means every action that validates. */
      requiredFor?: string[];
    }
  | {
      id: string;
      type: 'multiselect';
      label: string;
      options: FormOption[];
      default?: string[];
      min?: number;
      max?: number;
      /** Action ids that `min` applies to. Absent means every action that validates. */
      requiredFor?: string[];
    }
  | {
      id: string;
      type: 'text';
      label: string;
      multiline?: boolean;
      placeholder?: string;
      default?: string;
      required?: boolean;
      /** Action ids that `required` applies to. Absent means every action that validates. */
      requiredFor?: string[];
    };

export interface FormAction {
  id: string;
  label: string;
  description?: string;
  style?: ActionStyle;
  skipValidation?: boolean;
}

export interface FormRequest {
  title: string;
  body?: string;
  fields?: FormField[];
  actions: FormAction[];
}

export interface FormAnswer {
  action: string;
  values: Record<string, string | string[]>;
}
