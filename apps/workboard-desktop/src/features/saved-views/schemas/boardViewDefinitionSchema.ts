import { z } from "zod";

export const boardViewDefinitionSchema = z.object({
  id: z.uuid(),
  workspaceId: z.uuid(),
  title: z.string().trim().min(1, "Enter a title.").max(200),
  filters: z.object({
    query: z.string().trim().max(200).nullable(),
    repositoryIds: z.array(z.uuid()).max(100),
    statuses: z.array(z.enum(["backlog", "ready", "in_progress", "blocked", "review", "done", "cancelled"])).max(7),
  }),
  grouping: z.object({
    kind: z.enum(["hierarchy", "repository", "status"]),
    lanes: z.array(z.object({ key: z.string().min(1), title: z.string().min(1) })).max(32),
  }),
  sort: z.object({ field: z.enum(["title", "key"]), direction: z.enum(["ascending", "descending"]) }),
  density: z.enum(["comfortable", "compact"]),
  revision: z.number().int().nonnegative(),
});

export type ParsedBoardViewDefinition = z.infer<typeof boardViewDefinitionSchema>;
