import { describe, expect, it } from "vitest";

import type {
  BootstrapHandshake,
  ResponseEnvelope,
  SubscriptionMessage,
  SubscriptionReceipt,
} from "./generated";
import { createDaemonFacade, type DaemonTransport } from "./daemon";

interface Invocation {
  command: string;
  args?: Record<string, unknown>;
}

class FakeTransport implements DaemonTransport {
  readonly invocations: Invocation[] = [];
  readonly channels: Array<{ onmessage: (message: SubscriptionMessage) => void }> = [];
  readonly responses: unknown[] = [];

  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    this.invocations.push({ command, args });
    return Promise.resolve(this.responses.shift() as T);
  }

  channel<T>(): { onmessage: (message: T) => void } {
    const channel = { onmessage: (_message: T) => undefined };
    this.channels.push(
      channel as { onmessage: (message: SubscriptionMessage) => void },
    );
    return channel;
  }
}

const workspaceId = "20000000-0000-0000-0000-000000000001";
const featureId = "50000000-0000-0000-0000-000000000001";

describe("daemon facade", () => {
  it("is the typed boundary around all four IPC families", async () => {
    const transport = new FakeTransport();
    const facade = createDaemonFacade(transport);
    const handshake: BootstrapHandshake = {
      state: "read_only",
      subscriptions: [{ workspaceId }],
    };
    const response = { result: null } as ResponseEnvelope;
    const receipt: SubscriptionReceipt = { subscriptionId: 7 };
    transport.responses.push(handshake, response, response, response, response, response, receipt, receipt);

    await facade.handshake();
    await facade.workspaceSummary(workspaceId);
    await facade.hierarchyChildren(workspaceId, {
      kind: "workspace",
      id: workspaceId,
    });
    await facade.board(workspaceId, { cursor: null, limit: 200, query: null, repositoryIds: [], statuses: [], laneKeys: [], sort: { field: "key", direction: "ascending" } });
    await facade.attention(workspaceId, { cursor: null, limit: 200, repositoryIds: [], reasonCodes: [] });
    await facade.execute({
      workspaceId,
      expectedRevision: 41,
      idempotencyKey: "facade-test",
      command: {
        type: "approve_feature",
        value: { featureId },
      },
    });
    const subscription = await facade.subscribe(
      { workspaceId },
      null,
      () => undefined,
    );
    await subscription.cancel();

    expect(transport.invocations.map(({ command }) => command)).toEqual([
      "workboard_handshake",
      "workboard_query",
      "workboard_query",
      "workboard_query",
      "workboard_query",
      "workboard_execute",
      "workboard_subscribe",
      "workboard_subscribe",
    ]);
    expect(transport.invocations[1]?.args).toEqual({
      request: {
        workspaceId,
        query: { type: "workspace_summary" },
      },
    });
    expect(transport.invocations[3]?.args?.request).toMatchObject({ workspaceId, query: { type: "board" } });
    expect(transport.invocations[4]?.args?.request).toMatchObject({ workspaceId, query: { type: "attention" } });
    expect(transport.invocations[6]?.args?.request).toEqual({
      type: "start",
      value: { workspaceId, cursor: null },
    });
    expect(transport.invocations[7]?.args?.request).toEqual({
      type: "cancel",
      value: { subscriptionId: 7 },
    });
  });
});
