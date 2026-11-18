// Entry point for the schema generator (scripts/channels-schema.mjs). Each property is one payload
// a producer or the host exchanges. Not imported by the app.
import type {
  FormAnswer,
  FormRequest,
  GraphState,
  HostInfo,
  ReviewAnswer,
  ReviewRequest
} from '../src/lib/channels/types';

export interface ChannelPayloads {
  /** The reply to `pipan:hello`. */
  hostInfo: HostInfo;
  /** The `payload` of a `pipan:state` event whose primitive is `graph`. */
  graph: GraphState;
  /** The `payload` of a `pipan:request` with `kind: "form"`. */
  form: FormRequest;
  /** What `reply` receives for a form request. */
  formAnswer: FormAnswer;
  /** The `payload` of a `pipan:request` with `kind: "review"`. */
  review: ReviewRequest;
  /** What `reply` receives for a review request. */
  reviewAnswer: ReviewAnswer;
}
