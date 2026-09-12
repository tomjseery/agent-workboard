import { z } from "zod";

const text = z.string().trim().min(1, "This field is required.").max(8192, "Keep this field under 8,192 characters.");

export const workItemStateSchema = z.object({
  schemaVersion: z.literal(1),
  expectedStateRevision: z.number().int().nonnegative(),
  expectedDocumentRevision: z.number().int().positive(),
  currentState: text,
  nextAction: z.object({
    kind: z.enum(["actionable", "blocked", "paused", "review", "delivery"]),
    description: text,
  }),
  blockers: z.array(z.object({
    description: text,
    owner: text,
    unblockAction: text,
    resumeWhen: text,
  })).max(64),
  decisions: z.array(z.object({ decision: text, rationale: text })).max(64),
  verification: z.array(z.object({
    check: text,
    result: z.enum(["passed", "failed", "not_run"]),
    evidence: text.nullable(),
  })).max(64),
  review: z.object({
    status: z.enum(["not_started", "in_progress", "changes_requested", "ready", "accepted"]),
    evidence: z.array(text).max(64),
  }),
  delivery: z.object({
    status: z.enum(["not_started", "in_progress", "blocked", "ready", "delivered"]),
    evidence: z.array(text).max(64),
  }),
  status: z.enum(["backlog", "ready", "in_progress", "blocked", "review", "done", "cancelled"]),
  terminalIntent: z.enum(["complete", "cancel"]).nullable(),
}).superRefine((value, context) => {
  if (value.status === "blocked" && (value.blockers.length === 0 || value.nextAction.kind !== "blocked")) {
    context.addIssue({ code: "custom", path: ["status"], message: "Blocked status requires a blocker and a blocked next action." });
  }
  if (value.terminalIntent === "cancel" && value.status !== "cancelled") {
    context.addIssue({ code: "custom", path: ["terminalIntent"], message: "Cancel intent requires Cancelled status." });
  }
  if (value.terminalIntent === "complete" && value.status !== "review") {
    context.addIssue({ code: "custom", path: ["terminalIntent"], message: "Complete intent requires Review status until integration finishes." });
  }
});

export type WorkItemStateForm = z.input<typeof workItemStateSchema>;
