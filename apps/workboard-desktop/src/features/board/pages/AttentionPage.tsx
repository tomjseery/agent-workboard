import type { FeatureId, WorkItemId, WorkspaceId } from "../../../core/generated";
import { AttentionView } from "../components/AttentionView";

interface AttentionPageProps {
  workspaceId: WorkspaceId;
  onOpenWorkItem(workItemId: WorkItemId): void;
  onOpenFeature(featureId: FeatureId): void;
}

export function AttentionPage({ workspaceId, onOpenWorkItem, onOpenFeature }: AttentionPageProps) {
  return <section aria-labelledby="attention-title" className="space-y-5"><div><p className="text-sm text-[var(--muted-text)]">Daemon-ranked attention queue</p><h1 id="attention-title" className="text-2xl font-semibold">What needs me</h1></div><AttentionView workspaceId={workspaceId} onOpenWorkItem={onOpenWorkItem} onOpenFeature={onOpenFeature} /></section>;
}
