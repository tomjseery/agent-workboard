import { z } from "zod";

import type { WorkItemStatus } from "../../../core/contracts";

const text = z.string().trim().min(1, "This field is required.").max(8192, "Keep this field under 8,192 characters.");

const workItemStateSchema = z.object({
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
});

const transitions = {
  backlog: ["backlog", "ready", "in_progress", "blocked", "cancelled"],
  ready: ["ready", "in_progress", "blocked", "review", "cancelled"],
  in_progress: ["in_progress", "ready", "blocked", "review", "cancelled"],
  blocked: ["blocked", "ready", "in_progress", "review", "cancelled"],
  review: ["review", "in_progress", "blocked", "cancelled"],
  done: [],
  cancelled: [],
} as const satisfies Record<WorkItemStatus, readonly WorkItemStatus[]>;

export function allowedWorkItemStatuses(status: WorkItemStatus): readonly WorkItemStatus[] {
  return transitions[status];
}

export function createWorkItemStateSchema(startingStatus: WorkItemStatus) {
  return workItemStateSchema.superRefine((value, context) => {
    if (!(allowedWorkItemStatuses(startingStatus) as readonly WorkItemStatus[]).includes(value.status)) {
      context.addIssue({ code: "custom", path: ["status"], message: `Status cannot change from ${startingStatus.replaceAll("_", " ")} to ${value.status.replaceAll("_", " ")}.` });
    }
    if (value.status === "blocked" && (value.blockers.length === 0 || value.nextAction.kind !== "blocked")) {
      context.addIssue({ code: "custom", path: ["status"], message: "Blocked status requires a blocker and a blocked next action." });
    }
    if ((value.terminalIntent === "cancel") !== (value.status === "cancelled")) {
      context.addIssue({ code: "custom", path: ["terminalIntent"], message: "Cancel intent requires Cancelled status." });
    }
    if (value.terminalIntent === "complete" && value.status !== "review") {
      context.addIssue({ code: "custom", path: ["terminalIntent"], message: "Complete intent requires Review status until integration finishes." });
    }
  });
}

export type WorkItemStateForm = z.input<typeof workItemStateSchema>;
