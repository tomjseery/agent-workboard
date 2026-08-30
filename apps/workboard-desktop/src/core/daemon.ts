import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  BootstrapHandshake,
  EventCursor,
  ExecuteRequest,
  HierarchyRef,
  ResponseEnvelope,
  ResponseResult,
  SubscriptionMessage,
  SubscriptionTarget,
  WorkspaceId,
} from "./generated";

const commands = {
  handshake: "workboard_handshake",
  query: "workboard_query",
  execute: "workboard_execute",
  subscribe: "workboard_subscribe",
} as const;

interface MessageChannel<T> {
  onmessage: (message: T) => void;
}

export interface DaemonTransport {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  channel<T>(): MessageChannel<T>;
}

export interface DaemonSubscription {
  cancel(): Promise<void>;
}

type QueryResponse<TType extends ResponseResult["type"]> = Omit<
  ResponseEnvelope,
  "result"
> & {
  result: Extract<ResponseResult, { type: TType }> | null;
};

export interface DaemonFacade {
  handshake(): Promise<BootstrapHandshake>;
  workspaceSummary(workspaceId: WorkspaceId): Promise<QueryResponse<"workspace_summary">>;
  hierarchyChildren(
    workspaceId: WorkspaceId,
    parent: HierarchyRef,
  ): Promise<QueryResponse<"hierarchy_children">>;
  execute(request: ExecuteRequest): Promise<ResponseEnvelope>;
  subscribe(
    target: SubscriptionTarget,
    cursor: EventCursor | null,
    onMessage: (message: SubscriptionMessage) => void,
  ): Promise<DaemonSubscription>;
}

const tauriTransport: DaemonTransport = {
  invoke,
  channel: <T>() => new Channel<T>(),
};

export function createDaemonFacade(transport: DaemonTransport): DaemonFacade {
  const query = <TType extends ResponseResult["type"]>(
    workspaceId: WorkspaceId,
    read: Extract<import("./generated").ReadQuery, { type: TType }>,
  ) =>
    transport.invoke<QueryResponse<TType>>(commands.query, {
      request: { workspaceId, query: read },
    });

  return {
    handshake: () => transport.invoke<BootstrapHandshake>(commands.handshake),
    workspaceSummary: (workspaceId) =>
      query(workspaceId, { type: "workspace_summary" }),
    hierarchyChildren: (workspaceId, parent) =>
      query(workspaceId, { type: "hierarchy_children", value: { parent } }),
    execute: (request) =>
      transport.invoke<ResponseEnvelope>(commands.execute, { request }),
    subscribe: async (target, cursor, onMessage) => {
      const channel = transport.channel<SubscriptionMessage>();
      channel.onmessage = onMessage;
      const receipt = await transport.invoke<import("./generated").SubscriptionReceipt>(
        commands.subscribe,
        {
          request: {
            type: "start",
            value: { workspaceId: target.workspaceId, cursor },
          },
          onMessage: channel,
        },
      );
      return {
        cancel: async () => {
          const cancelChannel = transport.channel<SubscriptionMessage>();
          await transport.invoke(commands.subscribe, {
            request: {
              type: "cancel",
              value: { subscriptionId: receipt.subscriptionId },
            },
            onMessage: cancelChannel,
          });
        },
      };
    },
  };
}

export const daemon = createDaemonFacade(tauriTransport);
