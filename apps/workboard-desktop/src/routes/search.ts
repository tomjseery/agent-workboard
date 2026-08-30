import { z } from "zod";

export const hierarchySearchSchema = z.object({
  q: z.string().max(200).catch(""),
});

export type HierarchySearch = z.infer<typeof hierarchySearchSchema>;
