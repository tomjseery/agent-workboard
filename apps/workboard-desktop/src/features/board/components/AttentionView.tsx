import { useVirtualizer } from "@tanstack/react-virtual";
import { useRef } from "react";

import { Button } from "../../../components/ui/button";
import { Card } from "../../../components/ui/card";
import type { FeatureId, WorkItemId, WorkspaceId } from "../../../core/contracts";
import { useAttention } from "../hooks/useAttention";

interface AttentionViewProps {
  workspaceId: WorkspaceId;
  onOpenWorkItem(workItemId: WorkItemId): void;
  onOpenFeature(featureId: FeatureId): void;
}

export function AttentionView({ workspaceId, onOpenWorkItem, onOpenFeature }: AttentionViewProps) {
  const attention = useAttention(workspaceId);
  const scrollElement = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({ count: attention.entries.length, getScrollElement: () => scrollElement.current, estimateSize: () => 150, initialRect: { width: 1000, height: 800 }, overscan: 5, getItemKey: (index) => attention.entries[index]?.owner.id ?? index });
  if (attention.isLoading) return <p role="status">Loading What needs me…</p>;
  if (attention.isTransportError) return <p role="alert">What needs me could not be reached. Attention has not been classified locally.</p>;
  if (attention.error?.code === "projection_version_unavailable" || attention.error?.code === "incompatible_protocol") return <p role="alert">This daemon does not provide a compatible attention projection. No local queue has been inferred.</p>;
  if (attention.error != null) return <p role="alert">What needs me is unavailable: {attention.error.message}</p>;
  if (attention.totalCount === 0) return <p>Nothing currently requires your attention.</p>;

  return (
    <div className="space-y-3">
      {attention.isRefreshing && <p role="status">Refreshing authoritative attention evidence…</p>}
      {attention.isPartial && <p role="alert">Some attention evidence is partial.</p>}
      <div ref={scrollElement} role="list" aria-label="What needs me" className="h-[68vh] overflow-auto rounded-2xl border border-border">
        <div className="relative" style={{ height: `${virtualizer.getTotalSize()}px` }}>
          {virtualizer.getVirtualItems().map((item) => {
            const entry = attention.entries[item.index];
            if (entry === undefined) return null;
            return (
              <article
                key={`${entry.owner.kind}:${entry.owner.id}`}
                ref={virtualizer.measureElement}
                data-index={item.index}
                role="listitem"
                aria-posinset={entry.position}
                aria-setsize={entry.totalCount}
                className="absolute top-0 left-0 w-full p-2"
                style={{ transform: `translateY(${item.start}px)` }}
              >
                <Card size="compact">
                  <p className="text-xs font-semibold text-primary">{entry.subtitle}</p>
                  <h2 className="font-semibold">{entry.title}</h2>
                  <ul className="mt-2 list-disc pl-5 text-sm">{entry.reasons.map((reason) => <li key={reason.code}>{reason.message}</li>)}</ul>
                  <p className="mt-2 text-xs text-muted-foreground">{entry.repositories.map((repository) => repository.slug).join(", ")}</p>
                  {entry.card !== null && <Button type="button" onClick={() => onOpenWorkItem(entry.card!.workItem.id)} className="mt-3 py-1.5">Open Work item</Button>}
                  {entry.owner.kind === "feature" && <Button type="button" onClick={() => onOpenFeature(entry.owner.id)} className="mt-3 py-1.5">Review proposal</Button>}
                </Card>
              </article>
            );
          })}
        </div>
      </div>
      {attention.hasMore && (
        <Button type="button" size="lg" disabled={attention.isLoadingMore} onClick={() => void attention.loadMore()}>
          {attention.isLoadingMore ? "Loading more attention…" : `Load more of ${attention.totalCount}`}
        </Button>
      )}
    </div>
  );
}
