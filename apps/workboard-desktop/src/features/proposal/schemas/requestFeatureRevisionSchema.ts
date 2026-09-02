import { z } from "zod";

export const requestFeatureRevisionRequestSchema = z.object({
  feedback: z.string().trim().min(1, "Describe what the planner must change before requesting a revision."),
});

export type RequestFeatureRevisionRequest = z.infer<typeof requestFeatureRevisionRequestSchema>;
