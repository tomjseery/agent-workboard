import type { CheckoutId, WorkspaceId } from "../../../core/contracts";
import { CheckoutDetail } from "../components/CheckoutDetail";

export function CheckoutPage({ workspaceId, checkoutId }: { workspaceId: WorkspaceId; checkoutId: CheckoutId }) { return <CheckoutDetail workspaceId={workspaceId} checkoutId={checkoutId} />; }
