import { useQuery } from "@tanstack/react-query";

import { handshake } from "../../../core/bridge";

export const bootstrapQueryKey = ["workboard", "bootstrap"] as const;

export function useBootstrapQuery() {
  return useQuery({
    queryKey: bootstrapQueryKey,
    queryFn: handshake,
    retry: false,
  });
}
