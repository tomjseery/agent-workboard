import { useMutation, useQueryClient } from "@tanstack/react-query";

import type { BoardViewDefinition, WorkspaceId } from "../../../core/contracts";
import { savedViewQueryKeys } from "../api/savedViewQueryKeys";
import savedViewsApi from "../api/savedViewsApi";

export function useSaveBoardViewMutation(workspaceId: WorkspaceId, expectedRevision: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (definition: BoardViewDefinition) => savedViewsApi.save(workspaceId, expectedRevision, definition),
    onSuccess: (response) => {
      if (response.result?.type !== "board_view") return;
      const view = response.result.value;
      queryClient.setQueryData(savedViewQueryKeys.detail(workspaceId, view.id), { ...response, result: { type: "board_view", value: view } });
      void queryClient.invalidateQueries({ queryKey: savedViewQueryKeys.list(workspaceId) });
    },
  });
}
