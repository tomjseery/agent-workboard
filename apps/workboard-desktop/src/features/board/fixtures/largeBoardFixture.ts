import type { AttentionEntry, AttentionReasonCode, WorkItemCard, BoardLane, Provider, RepositoryReference, WorkItemStatus, WorkspaceId } from "../../../core/contracts";

const workspaceId = "20000000-0000-0000-0000-000000000001" as WorkspaceId;
const statuses: WorkItemStatus[] = ["backlog", "ready", "in_progress", "blocked", "review", "done", "cancelled"];
const attentionCodes: AttentionReasonCode[] = ["approval_required", "revision_requested", "reconciliation_required", "blocked", "checkpoint_due", "interrupted_operation", "recovery_conflict", "stale_or_unknown_session"];

function identity(prefix: string, index: number) {
  return `${prefix}0000000-0000-0000-0000-${index.toString(16).padStart(12, "0")}`;
}

export function createLargeBoardFixture() {
  const repositories: RepositoryReference[] = Array.from({ length: 100 }, (_, index) => ({ id: identity("3", index + 1), workspaceId, slug: `service-${index.toString().padStart(3, "0")}`, title: `Service ${index}` }));
  const lanes: BoardLane[] = statuses.map((status, index) => ({ key: status, title: status.replaceAll("_", " "), position: index + 1, totalCount: 0 }));
  const cards: WorkItemCard[] = [];
  const laneCounts = new Map<WorkItemStatus, number>();
  for (let index = 0; index < 10_000; index += 1) {
    const featureIndex = Math.floor(index / 10);
    const itemInFeature = index % 10;
    const status = statuses[index % statuses.length]!;
    const lanePosition = (laneCounts.get(status) ?? 0) + 1;
    laneCounts.set(status, lanePosition);
    const provider: Provider = index % 2 === 0 ? "claude" : "codex";
    const attentionCode = attentionCodes[index % attentionCodes.length]!;
    const workItemId = identity("6", index + 1);
    const dependencyId = identity("6", Math.max(1, index));
    const repositoryScope = [repositories[index % repositories.length]!, repositories[(index + 17) % repositories.length]!];
    cards.push({
      workItem: { id: workItemId, featureId: identity("5", featureIndex + 1), key: `F${featureIndex.toString().padStart(4, "0")}/WI${itemInFeature}`, slug: `work-item-${index}`, title: `Work item ${index}` },
      feature: { id: identity("5", featureIndex + 1), epicId: identity("4", Math.floor(featureIndex / 10) + 1), slug: `feature-${featureIndex}`, title: `Feature ${featureIndex}` },
      status,
      laneKey: status,
      lanePosition,
      laneCount: 0,
      dependencyReadiness: itemInFeature === 0 ? "ready" : itemInFeature % 4 === 0 ? "blocked" : "waiting",
      blockedBy: itemInFeature === 0 ? [] : [{ workItem: { id: dependencyId, featureId: identity("5", featureIndex + 1), key: `F${featureIndex.toString().padStart(4, "0")}/WI${itemInFeature - 1}`, slug: `work-item-${index - 1}`, title: `Work item ${index - 1}` }, status: itemInFeature % 4 === 0 ? "blocked" : "in_progress" }],
      parallelReadiness: { groupKey: `feature-${featureIndex}-group-${itemInFeature % 3}`, readyCount: itemInFeature === 0 ? 4 : 0, waitingCount: itemInFeature === 0 ? 0 : 3 },
      repositories: repositoryScope,
      sessionSummary: { total: 1, active: index % 5 === 0 ? 1 : 0, idle: index % 5 === 1 ? 1 : 0, unknown: index % 5 > 1 ? 1 : 0, providers: [provider] },
      checkoutIds: [identity("7", index + 1)],
      sessionIds: [identity("8", index + 1)],
      attentionReasons: index % 3 === 0 ? [{ code: attentionCode, rank: (index % attentionCodes.length) + 1, message: `Authoritative ${attentionCode.replaceAll("_", " ")}` }] : [],
      revision: index + 1,
      availableActions: [],
    });
  }
  for (const card of cards) card.laneCount = laneCounts.get(card.status) ?? 0;
  for (const lane of lanes) lane.totalCount = laneCounts.get(lane.key as WorkItemStatus) ?? 0;
  const attentionEntries: AttentionEntry[] = cards.filter((card) => card.attentionReasons.length > 0).map((card, index, source) => ({ owner: { kind: "work_item", id: card.workItem.id }, title: card.workItem.title, subtitle: card.workItem.key, repositories: card.repositories, card, reasons: card.attentionReasons, revision: card.revision, availableActions: [], position: index + 1, totalCount: source.length }));
  return { workspaceId, repositories, features: 1_000, lanes, cards, attentionEntries };
}
