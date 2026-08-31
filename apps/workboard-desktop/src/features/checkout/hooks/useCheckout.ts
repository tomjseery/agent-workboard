import type { CheckoutId, WorkspaceId } from "../../../core/generated";
import { useCheckoutQuery } from "./useCheckoutQuery";

export function useCheckout(workspaceId: WorkspaceId, checkoutId: CheckoutId) {
  const query = useCheckoutQuery(workspaceId, checkoutId);
  return { projection: query.data?.result?.type === "checkout_observability" ? query.data.result.value : undefined, error: query.data?.error, isLoading: query.isPending, isRefreshing: query.isFetching && !query.isPending, isDisconnected: query.isError, isPartial: (query.data?.partialOutcomes.length ?? 0) > 0, retry: query.refetch };
}
