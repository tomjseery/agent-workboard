import type { BootstrapState, WorkspaceId } from "../../../core/generated";
import { useBootstrapQuery } from "./useBootstrapQuery";
import { useBootstrapSubscription } from "./useBootstrapSubscription";

function refusalOf(error: unknown): string | undefined {
  if (typeof error !== "object" || error === null || !("message" in error)) return undefined;
  const message = (error as { message?: unknown }).message;
  return typeof message === "string" && message.length > 0 ? message : undefined;
}

export function useBootstrap(): { state: BootstrapState; workspaceId?: WorkspaceId; refusal?: string } {
  const query = useBootstrapQuery();
  const target = query.data?.subscriptions[0];
  useBootstrapSubscription(target);

  if (query.isPending) {
    return { state: "connecting" };
  }
  if (query.isError || query.data === undefined) {
    return { state: "disconnected", refusal: refusalOf(query.error) };
  }
  return {
    state: query.data.state,
    workspaceId: query.data.subscriptions[0]?.workspaceId,
    refusal: query.data.refusal ?? undefined,
  };
}
