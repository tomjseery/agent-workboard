import { useVirtualizer } from "@tanstack/react-virtual";
import { useRef } from "react";

import type { FeatureId, WorkItemId, WorkspaceId } from "../../../core/generated";
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
      <div ref={scrollElement} role="list" aria-label="What needs me" className="h-[68vh] overflow-auto rounded-2xl border border-[var(--border)]">
        <div className="relative" style={{ height: `${virtualizer.getTotalSize()}px` }}>
          {virtualizer.getVirtualItems().map((item) => { const entry = attention.entries[item.index]; if (entry === undefined) return null; return <article key={`${entry.owner.kind}:${entry.owner.id}`} ref={virtualizer.measureElement} data-index={item.index} role="listitem" aria-posinset={entry.position} aria-setsize={entry.totalCount} className="absolute left-0 top-0 w-full p-2" style={{ transform: `translateY(${item.start}px)` }}><div className="rounded-xl border border-[var(--border)] bg-[var(--surface)] p-4"><p className="text-xs font-semibold text-[var(--accent)]">{entry.subtitle}</p><h2 className="font-semibold">{entry.title}</h2><ul className="mt-2 list-disc pl-5 text-sm">{entry.reasons.map((reason) => <li key={reason.code}>{reason.message}</li>)}</ul><p className="mt-2 text-xs text-[var(--muted-text)]">{entry.repositories.map((repository) => repository.slug).join(", ")}</p>{entry.card !== null && <button type="button" onClick={() => onOpenWorkItem(entry.card!.workItem.id)} className="mt-3 rounded-lg border border-[var(--border)] px-3 py-1.5">Open Work item</button>}{entry.owner.kind === "feature" && <button type="button" onClick={() => onOpenFeature(entry.owner.id)} className="mt-3 rounded-lg border border-[var(--border)] px-3 py-1.5">Review proposal</button>}</div></article>; })}
        </div>
      </div>
      {attention.hasMore && <button type="button" disabled={attention.isLoadingMore} onClick={() => void attention.loadMore()} className="rounded-lg border border-[var(--border)] px-4 py-2">{attention.isLoadingMore ? "Loading more attention…" : `Load more of ${attention.totalCount}`}</button>}
    </div>
  );
}
