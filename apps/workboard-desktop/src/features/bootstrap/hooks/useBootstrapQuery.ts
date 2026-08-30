import { useQuery } from "@tanstack/react-query";

import { daemon } from "../../../core/daemon";

export const bootstrapQueryKey = ["workboard", "bootstrap"] as const;

export function useBootstrapQuery() {
  return useQuery({
    queryKey: bootstrapQueryKey,
    queryFn: daemon.handshake,
    retry: false,
  });
}
