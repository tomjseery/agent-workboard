import { useInfiniteQuery } from "@tanstack/react-query";

import type { AttentionQuery, WorkspaceId } from "../../../core/generated";
import { boardApi } from "../api/boardApi";
import { boardQueryKeys } from "../api/boardQueryKeys";

export function useAttentionQuery(workspaceId: WorkspaceId, parameters: Omit<AttentionQuery, "cursor">) {
  return useInfiniteQuery({
    queryKey: boardQueryKeys.attentionList(workspaceId, parameters),
    queryFn: ({ pageParam }) => boardApi.attention(workspaceId, { ...parameters, cursor: pageParam }),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.result?.type === "attention" ? page.result.value.nextCursor ?? undefined : undefined,
  });
}
