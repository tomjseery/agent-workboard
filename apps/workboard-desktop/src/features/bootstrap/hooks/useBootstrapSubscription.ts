import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { cancelSubscription, subscribe } from "../../../core/bridge";
import type {
  BootstrapHandshake,
  BootstrapState,
  SubscriptionMessage,
  SubscriptionTarget,
} from "../types/bootstrap";
import { bootstrapQueryKey } from "./useBootstrapQuery";

const ignoreBridgeFailure = () => undefined;

const stateResolvers: Record<
  SubscriptionMessage["type"],
  (message: SubscriptionMessage, current: BootstrapState) => BootstrapState
> = {
  connected: (message) =>
    message.type === "connected" ? message.value.state : "disconnected",
  event: (_message, current) => current,
  resyncing: () => "resyncing",
  resynced: (_message, current) =>
    current === "resyncing" ? "read_only" : current,
  disconnected: () => "disconnected",
  incompatible: () => "incompatible",
};

export function resolveSubscriptionState(
  message: SubscriptionMessage,
  current: BootstrapState,
): BootstrapState {
  return stateResolvers[message.type](message, current);
}

export function useBootstrapSubscription(target: SubscriptionTarget | undefined) {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (target === undefined) {
      return;
    }

    let disposed = false;
    let receipt: Awaited<ReturnType<typeof subscribe>> | undefined;
    const onMessage = (message: SubscriptionMessage) => {
      queryClient.setQueryData<BootstrapHandshake>(bootstrapQueryKey, (current) => {
        if (current === undefined) {
          return current;
        }
        return {
          ...current,
          state: resolveSubscriptionState(message, current.state),
        };
      });
    };

    void subscribe(target, onMessage)
      .then((started) => {
        receipt = started;
        if (disposed) {
          void cancelSubscription(started.subscriptionId).catch(ignoreBridgeFailure);
        }
      })
      .catch(() => {
        if (!disposed) {
          queryClient.setQueryData<BootstrapHandshake>(bootstrapQueryKey, (current) =>
            current === undefined ? current : { ...current, state: "disconnected" },
          );
        }
      });

    return () => {
      disposed = true;
      if (receipt !== undefined) {
        void cancelSubscription(receipt.subscriptionId).catch(ignoreBridgeFailure);
      }
    };
  }, [queryClient, target]);
}
