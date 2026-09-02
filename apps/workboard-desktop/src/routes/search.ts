import { z } from "zod";

export const repositoryViewSchema = z.object({
  view: z.enum(["board", "features", "evidence"]).catch("board").default("board"),
});

export const epicViewSchema = z.object({
  view: z.enum(["board", "features"]).catch("board").default("board"),
});

export const featureTabSchema = z.object({
  tab: z.enum(["board", "detail", "proposal"]).catch("board").default("board"),
});

export type RepositorySearch = z.infer<typeof repositoryViewSchema>;
export type EpicSearch = z.infer<typeof epicViewSchema>;
export type FeatureSearch = z.infer<typeof featureTabSchema>;
