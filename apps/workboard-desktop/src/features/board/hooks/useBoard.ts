import type { BoardCardProjection, WorkspaceId } from "../../../core/generated";
import { useBoardInteractionStore } from "../store/boardInteractionStore";
import { useBoardQuery } from "./useBoardQuery";

export function useBoard(workspaceId: WorkspaceId) {
  const filters = useBoardInteractionStore((state) => state.filters);
  const selectedWorkItemId = useBoardInteractionStore((state) => state.selectedWorkItemId);
  const focusedWorkItemId = useBoardInteractionStore((state) => state.focusedWorkItemId);
  const select = useBoardInteractionStore((state) => state.select);
  const focus = useBoardInteractionStore((state) => state.focus);
  const setQuery = useBoardInteractionStore((state) => state.setQuery);
  const toggleRepository = useBoardInteractionStore((state) => state.toggleRepository);
  const toggleStatus = useBoardInteractionStore((state) => state.toggleStatus);
  const setLaneKeys = useBoardInteractionStore((state) => state.setLaneKeys);
  const setSort = useBoardInteractionStore((state) => state.setSort);
  const resetFilters = useBoardInteractionStore((state) => state.resetFilters);
  const query = useBoardQuery(workspaceId, {
    limit: 200,
    query: filters.query.trim() === "" ? null : filters.query,
    repositoryIds: filters.repositoryIds,
    statuses: filters.statuses,
    laneKeys: filters.laneKeys,
    sort: filters.sort,
  });
  const envelopes = query.data?.pages ?? [];
  const projections = envelopes.flatMap((page) => page.result?.type === "board" ? [page.result.value] : []);
  const cardsByLane = new Map<string, BoardCardProjection[]>();
  for (const card of projections.flatMap((page) => page.cards)) {
    const lane = cardsByLane.get(card.laneKey) ?? [];
    lane.push(card);
    cardsByLane.set(card.laneKey, lane);
  }
  for (const lane of cardsByLane.values()) lane.sort((left, right) => left.lanePosition - right.lanePosition);
  const lanes = projections[0]?.lanes ?? [];
  const visibleLanes = lanes.filter((lane) => filters.laneKeys.length === 0 || filters.laneKeys.includes(lane.key));
  const repositories = new Map<string, BoardCardProjection["repositories"][number]>();
  for (const cards of cardsByLane.values()) for (const card of cards) for (const repository of card.repositories) repositories.set(repository.id, repository);
  const move = (card: BoardCardProjection, key: string) => {
    const laneIndex = visibleLanes.findIndex((lane) => lane.key === card.laneKey);
    const cards = cardsByLane.get(card.laneKey) ?? [];
    const cardIndex = cards.findIndex((candidate) => candidate.workItem.id === card.workItem.id);
    if (laneIndex < 0 || cardIndex < 0) return;
    if (key === "ArrowLeft" || key === "ArrowRight") {
      const nextLane = visibleLanes[laneIndex + (key === "ArrowLeft" ? -1 : 1)];
      const nextCards = nextLane === undefined ? [] : cardsByLane.get(nextLane.key) ?? [];
      const nextCard = nextCards[Math.min(cardIndex, Math.max(0, nextCards.length - 1))];
      if (nextCard !== undefined) focus(nextCard.workItem.id);
      return;
    }
    const nextIndex = key === "Home" ? 0 : key === "End" ? cards.length - 1 : Math.max(0, Math.min(cards.length - 1, cardIndex + (key === "ArrowUp" ? -1 : key === "ArrowDown" ? 1 : key === "PageUp" ? -10 : 10)));
    const nextCard = cards[nextIndex];
    if (nextCard !== undefined) focus(nextCard.workItem.id);
  };
  const toggleLane = (key: string) => {
    const all = lanes.map((lane) => lane.key);
    const current = filters.laneKeys.length === 0 ? all : filters.laneKeys;
    const next = current.includes(key) ? current.filter((candidate) => candidate !== key) : [...current, key];
    setLaneKeys(next.length === all.length ? [] : next);
  };
  return {
    lanes,
    visibleLanes,
    cardsByLane,
    repositories: [...repositories.values()].sort((left, right) => left.slug.localeCompare(right.slug)),
    totalCount: projections[0]?.totalCount ?? 0,
    selectedWorkItemId,
    focusedWorkItemId,
    filters,
    select,
    focus,
    setQuery,
    toggleRepository,
    toggleStatus,
    setLaneKeys,
    setSort,
    resetFilters,
    move,
    toggleLane,
    loadMore: query.fetchNextPage,
    hasMore: query.hasNextPage,
    isLoadingMore: query.isFetchingNextPage,
    isLoading: query.isPending,
    isRefreshing: query.isFetching && !query.isPending && !query.isFetchingNextPage,
    isPartial: envelopes.some((page) => page.partialOutcomes.length > 0),
    error: envelopes.find((page) => page.error !== null)?.error,
    isTransportError: query.isError,
  };
}
