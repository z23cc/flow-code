// WebSocket event types matching Rust FlowEvent #[serde(tag="type", content="data")]

export interface TaskStatusChanged {
  task_id: string;
  epic_id: string;
  from_status: string;
  to_status: string;
}

export interface DagMutated {
  mutation: string;
  details: unknown;
}

export interface AgentLog {
  agent_id: string;
  task_id: string;
  level: string;
  message: string;
}

export interface EpicUpdated {
  epic_id: string;
  field: string;
  value: unknown;
}

// Heartbeat has no data (content is null in serde)
export type Heartbeat = null;

export interface ApprovalCreated {
  id: string;
  task_id: string;
}

export interface ApprovalResolved {
  id: string;
  status: string;
}

export type FlowEventType =
  | "TaskStatusChanged"
  | "DagMutated"
  | "AgentLog"
  | "EpicUpdated"
  | "Heartbeat"
  | "ApprovalCreated"
  | "ApprovalResolved";

export type FlowEventData =
  | TaskStatusChanged
  | DagMutated
  | AgentLog
  | EpicUpdated
  | Heartbeat
  | ApprovalCreated
  | ApprovalResolved;

/** Envelope sent over WebSocket: TimestampedEvent from Rust */
export interface TimestampedEvent {
  timestamp: string;
  event: {
    type: FlowEventType;
    data?: FlowEventData;
  };
}

/** Generic API response wrapper */
export interface ApiResponse<T> {
  data: T;
}

/** API error shape returned by the daemon */
export interface ApiError {
  error: string;
  status: number;
}
