import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  BootstrapHandshake,
  SubscriptionMessage,
  SubscriptionReceipt,
  SubscriptionTarget,
} from "../features/bootstrap/types/bootstrap";

const commands = {
  handshake: "workboard_handshake",
  subscribe: "workboard_subscribe",
} as const;

export function handshake(): Promise<BootstrapHandshake> {
  return invoke<BootstrapHandshake>(commands.handshake);
}

export async function subscribe(
  target: SubscriptionTarget,
  onMessage: (message: SubscriptionMessage) => void,
): Promise<SubscriptionReceipt> {
  const channel = new Channel<SubscriptionMessage>();
  channel.onmessage = onMessage;
  return invoke<SubscriptionReceipt>(commands.subscribe, {
    request: {
      type: "start",
      value: {
        workspaceId: target.workspaceId,
        cursor: null,
      },
    },
    onMessage: channel,
  });
}

export function cancelSubscription(subscriptionId: number): Promise<SubscriptionReceipt> {
  const channel = new Channel<SubscriptionMessage>();
  return invoke<SubscriptionReceipt>(commands.subscribe, {
    request: {
      type: "cancel",
      value: { subscriptionId },
    },
    onMessage: channel,
  });
}
