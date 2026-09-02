import { useQuery } from "@tanstack/react-query";
import type { CheckoutId, WorkspaceId } from "../../../core/contracts";
import checkoutApi from "../api/checkoutApi";
import { checkoutQueryKeys } from "../api/checkoutQueryKeys";

export function useCheckoutQuery(workspaceId: WorkspaceId, checkoutId: CheckoutId) {
  return useQuery({ queryKey: checkoutQueryKeys.detail(workspaceId, checkoutId), queryFn: () => checkoutApi.get(workspaceId, checkoutId) });
}
