import type { ComposerTarget } from '#lib/channels/diff-rows.ts';

/** Interface state shared by the review views. It lives above the virtual list so scrolling a row out of view keeps a draft. */
export interface ReviewUi {
  /** Where the new-comment box is open, if anywhere. */
  composer: ComposerTarget | null;
  /** Text typed into the new-comment box. */
  draft: string;
  /** The comment being edited and its current text. */
  editing: { id: string; text: string } | null;
}

export const emptyUi = (): ReviewUi => ({ composer: null, draft: '', editing: null });

/** Svelte action: focuses the element when it appears. */
export function focusOnMount(node: HTMLElement) {
  node.focus();
}
