export type BootstrapState =
  | "connecting"
  | "disconnected"
  | "incompatible"
  | "read_only"
  | "resyncing"
  | "ready";

export interface SubscriptionTarget {
  workspaceId: string;
}

export interface BootstrapHandshake {
  state: BootstrapState;
  subscriptions: SubscriptionTarget[];
}

export interface SubscriptionReceipt {
  subscriptionId: number;
}

interface ConnectedMessage {
  type: "connected";
  value: {
    state: "read_only" | "ready";
  };
}

interface EventMessage {
  type: "event";
  value: unknown;
}

interface ResyncingMessage {
  type: "resyncing";
  value: unknown;
}

interface ResyncedMessage {
  type: "resynced";
  value: unknown;
}

interface DisconnectedMessage {
  type: "disconnected";
  value: {
    code: string;
  };
}

interface IncompatibleMessage {
  type: "incompatible";
}

export type SubscriptionMessage =
  | ConnectedMessage
  | EventMessage
  | ResyncingMessage
  | ResyncedMessage
  | DisconnectedMessage
  | IncompatibleMessage;
