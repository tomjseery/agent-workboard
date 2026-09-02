import { Link } from "@tanstack/react-router";
import { Ban } from "lucide-react";
import { memo } from "react";

import { Badge } from "../../../components/ui/badge";
import { Card } from "../../../components/ui/card";
import type { WorkItemCard, WorkspaceId } from "../../../core/contracts";
import { dependencyReadinessPresentations } from "../model/presentation";

interface BoardCardProps {
  card: WorkItemCard;
  workspaceId: WorkspaceId;
  evidenceLinks: boolean;
  selected: boolean;
  focused: boolean;
  onSelect(): void;
  onFocus(): void;
  onOpen(): void;
  onKeyDown(event: React.KeyboardEvent<HTMLButtonElement>): void;
}

export const BoardCard = memo(function BoardCard({ card, workspaceId, evidenceLinks, selected, focused, onSelect, onFocus, onOpen, onKeyDown }: BoardCardProps) {
  const readiness = dependencyReadinessPresentations[card.dependencyReadiness];
  const ReadinessIcon = readiness.icon;
  return (
    <article role="listitem" aria-posinset={card.lanePosition} aria-setsize={card.laneCount} className="px-2 py-1">
      <Card asChild size="inset" className="w-full rounded-xl text-left focus-visible:ring-2 focus-visible:ring-ring">
        <button
          type="button"
          data-board-card={card.workItem.id}
          tabIndex={focused ? 0 : -1}
          aria-current={selected ? "true" : undefined}
          aria-label={`${card.workItem.key}: ${card.workItem.title}. Position ${card.lanePosition} of ${card.laneCount} in ${card.laneKey}.`}
          onClick={onSelect}
          onDoubleClick={onOpen}
          onFocus={onFocus}
          onKeyDown={onKeyDown}
        >
          <span className="block text-xs font-semibold text-primary">{card.workItem.key}</span>
          <span className="mt-1 block font-medium">{card.workItem.title}</span>
          <span className="mt-1 block text-xs text-muted-foreground">{card.feature.title}</span>
          <span className="mt-3 flex flex-wrap gap-1">
            {card.repositories.map((repository) => <Badge key={repository.id} size="tag">{repository.slug}</Badge>)}
          </span>
          <span className="mt-2 flex flex-wrap items-center gap-1">
            <Badge tone={readiness.tone}><ReadinessIcon className="size-3" aria-hidden />{readiness.label}</Badge>
            {card.blockedBy.length > 0 && <Badge tone="warning"><Ban className="size-3" aria-hidden />blocked by {card.blockedBy.map((evidence) => evidence.workItem.key).join(", ")}</Badge>}
          </span>
          <span className="mt-1 block text-xs">Parallel: {card.parallelReadiness.readyCount} ready, {card.parallelReadiness.waitingCount} waiting</span>
          <span className="mt-1 block text-xs">Sessions: {card.sessionSummary.total}</span>
          {card.attentionReasons.length > 0 && <span className="mt-2 block text-xs font-semibold text-warning">{card.attentionReasons.map((reason) => reason.message).join(" · ")}</span>}
        </button>
      </Card>
      {evidenceLinks && (card.checkoutIds.length > 0 || card.sessionIds.length > 0) && (
        <nav aria-label={`Evidence for ${card.workItem.key}`} className="mt-1 flex flex-wrap gap-2 px-1">
          {card.checkoutIds.map((checkoutId) => (
            <Badge key={checkoutId} size="tag" asChild>
              <Link to="/workspaces/$workspaceId/checkouts/$checkoutId" params={{ workspaceId, checkoutId }}>Checkout {checkoutId.slice(0, 8)}</Link>
            </Badge>
          ))}
          {card.sessionIds.map((sessionId) => (
            <Badge key={sessionId} size="tag" asChild>
              <Link to="/workspaces/$workspaceId/sessions/$sessionId" params={{ workspaceId, sessionId }}>Session {sessionId.slice(0, 8)}</Link>
            </Badge>
          ))}
        </nav>
      )}
    </article>
  );
});
