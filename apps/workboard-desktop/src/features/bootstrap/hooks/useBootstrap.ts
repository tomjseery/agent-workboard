import type { BootstrapState, WorkspaceId } from "../../../core/generated";
import { useBootstrapQuery } from "./useBootstrapQuery";
import { useBootstrapSubscription } from "./useBootstrapSubscription";

export function useBootstrap(): { state: BootstrapState; workspaceId?: WorkspaceId } {
  const query = useBootstrapQuery();
  const target = query.data?.subscriptions[0];
  useBootstrapSubscription(target);

  if (query.isPending) {
    return { state: "connecting" };
  }
  if (query.isError || query.data === undefined) {
    return { state: "disconnected" };
  }
  return { state: query.data.state, workspaceId: query.data.subscriptions[0]?.workspaceId };
}
