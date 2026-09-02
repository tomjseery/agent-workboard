import { useInfiniteQuery } from "@tanstack/react-query";

import type { BoardQuery, WorkspaceId } from "../../../core/contracts";
import { boardApi } from "../api/boardApi";
import { boardQueryKeys } from "../api/boardQueryKeys";

export function useBoardQuery(workspaceId: WorkspaceId, parameters: Omit<BoardQuery, "cursor">) {
  return useInfiniteQuery({
    queryKey: boardQueryKeys.board(workspaceId, parameters),
    queryFn: ({ pageParam }) => boardApi.page(workspaceId, { ...parameters, cursor: pageParam }),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.result?.type === "board" ? page.result.value.nextCursor ?? undefined : undefined,
  });
}
