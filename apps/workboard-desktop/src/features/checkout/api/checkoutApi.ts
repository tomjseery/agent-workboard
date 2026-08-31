import { daemon } from "../../../core/daemon";
import type { CheckoutId, WorkspaceId } from "../../../core/generated";

const checkoutApi = { get: (workspaceId: WorkspaceId, checkoutId: CheckoutId) => daemon.checkoutObservability(workspaceId, checkoutId) };
export default checkoutApi;
