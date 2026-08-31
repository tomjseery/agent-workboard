import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef } from "react";

import type { BoardCardProjection, BoardLaneProjection, WorkItemId, WorkspaceId } from "../../../core/generated";
import { BoardCard } from "./BoardCard";

interface VirtualLaneProps {
  lane: BoardLaneProjection;
  workspaceId: WorkspaceId;
  evidenceLinks: boolean;
  cards: BoardCardProjection[];
  selectedWorkItemId?: WorkItemId;
  focusedWorkItemId?: WorkItemId;
  onSelect(card: BoardCardProjection): void;
  onFocus(card: BoardCardProjection): void;
  onOpen(card: BoardCardProjection): void;
  onMove(card: BoardCardProjection, key: string): void;
}

export function VirtualLane({ lane, workspaceId, evidenceLinks, cards, selectedWorkItemId, focusedWorkItemId, onSelect, onFocus, onOpen, onMove }: VirtualLaneProps) {
  const scrollElement = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({ count: cards.length, getScrollElement: () => scrollElement.current, estimateSize: () => 210, initialRect: { width: 320, height: 800 }, overscan: 4, getItemKey: (index) => cards[index]?.workItem.id ?? index });
  const focusedIndex = cards.findIndex((card) => card.workItem.id === focusedWorkItemId);
  useEffect(() => {
    if (focusedIndex < 0) return;
    virtualizer.scrollToIndex(focusedIndex, { align: "auto" });
    requestAnimationFrame(() => scrollElement.current?.querySelector<HTMLElement>(`[data-board-card="${focusedWorkItemId}"]`)?.focus());
  }, [focusedIndex, focusedWorkItemId, virtualizer]);

  return (
    <section aria-labelledby={`lane-${lane.key}`} className="flex min-w-72 max-w-80 flex-1 flex-col rounded-2xl border border-[var(--border)] bg-[var(--surface-muted)]">
      <h2 id={`lane-${lane.key}`} className="flex items-center justify-between px-4 py-3 text-sm font-semibold"><span>{lane.title}</span><span aria-label={`${lane.totalCount} cards`}>{lane.totalCount}</span></h2>
      <div ref={scrollElement} role="list" aria-label={`${lane.title} cards`} className="h-[62vh] min-h-80 overflow-auto">
        <div className="relative w-full" style={{ height: `${virtualizer.getTotalSize()}px` }}>
          {virtualizer.getVirtualItems().map((item) => {
            const card = cards[item.index];
            if (card === undefined) return null;
            return <div key={card.workItem.id} ref={virtualizer.measureElement} data-index={item.index} className="absolute left-0 top-0 w-full" style={{ transform: `translateY(${item.start}px)` }}><BoardCard card={card} workspaceId={workspaceId} evidenceLinks={evidenceLinks} selected={selectedWorkItemId === card.workItem.id} focused={focusedWorkItemId === card.workItem.id || (focusedWorkItemId === undefined && item.index === 0 && lane.position === 1)} onSelect={() => onSelect(card)} onFocus={() => onFocus(card)} onOpen={() => onOpen(card)} onKeyDown={(event) => { if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown", "Enter", " "].includes(event.key)) { event.preventDefault(); if (event.key === "Enter") onOpen(card); else if (event.key === " ") onSelect(card); else onMove(card, event.key); } }} /></div>;
          })}
        </div>
      </div>
    </section>
  );
}
