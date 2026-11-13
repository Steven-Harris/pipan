/** Host-enforced limits from docs/plans/extension-channel-spec.md section 3, plus store bounds. */

export const MAX_PAYLOAD_BYTES = 8 * 1024 * 1024;
export const MAX_GRAPH_NODES = 500;
export const MAX_GRAPH_EDGES = 2000;
export const MAX_REVIEW_FILES = 200;
export const MAX_REVIEW_DIFF_LINES = 50_000;
export const MAX_REVIEW_COMMENTS = 1000;

/** Pending requests per session. Requests beyond this are cancelled on arrival. */
export const MAX_PENDING_REQUESTS = 20;
/** Requests accepted per session within `REQUEST_RATE_WINDOW_MS`. */
export const MAX_REQUESTS_PER_WINDOW = 60;
export const REQUEST_RATE_WINDOW_MS = 60_000;

export const MAX_PRODUCER_ERRORS = 50;
export const MAX_LOG_ITEMS = 1000;
