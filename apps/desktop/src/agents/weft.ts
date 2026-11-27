// What `weft_state` returns (src-tauri/src/agents.rs `WeftView`, ADR-0058): the open project's
// WEFT journal (ADR-0053) replayed, with the time and revision the journal recorded for each item.
// The shapes must match `eak_harness::weft` and the view structs in agents.rs.

export type WeftTaskStatus = "pending" | "in_progress" | "blocked" | "done" | "rejected";
export type WeftMessageKind = "delegate" | "context_share" | "status" | "finding" | "review_request" | "review_verdict" | "blocker";
export type WeftVerdict = "approve" | "request_changes" | "reject";

/** `weft::AgentCard`: what the agent's launcher declared (EAK does not verify the model). */
export interface WeftCard {
  id: string;
  name: string;
  session_id: string;
  tool: string;
  model: string;
  capabilities: string[];
}

/** `weft::Task`. */
export interface WeftTask {
  id: string;
  title: string;
  owner: string;
  created_by: string;
  depends_on: string[];
  parent: string | null;
  status: WeftTaskStatus;
}

/** `weft::Message`. `to` is an agent id, "engineer" or "*" (everyone). */
export interface WeftMessage {
  id: string;
  kind: WeftMessageKind;
  from: string;
  to: string;
  task_id: string | null;
  proposal_id: string | null;
  body: string;
  verdict: WeftVerdict | null;
  status: WeftTaskStatus | null;
}

export interface WeftAgentView {
  card: WeftCard;
  registered_ms: number;
  registered_revision: number;
}

export interface WeftTaskView {
  task: WeftTask;
  created_ms: number;
  created_revision: number;
  updated_ms: number;
  waiting_on: string[];
  ready: boolean;
}

export interface WeftMessageView {
  seq: number;
  ts_ms: number;
  revision: number;
  message: WeftMessage;
}

export interface WeftView {
  project_dir: string;
  revision: number;
  entries: number;
  unreadable: number;
  agents: WeftAgentView[];
  tasks: WeftTaskView[];
  messages: WeftMessageView[];
}
