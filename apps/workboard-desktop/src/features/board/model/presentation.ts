import { Ban, Check, Hourglass, type LucideIcon } from "lucide-react";

import type { BadgeTone } from "../../../components/ui/badge";
import type { DependencyReadiness, WorkItemStatus } from "../../../core/contracts";

export interface LanePresentation {
  title: string;
  hiddenByDefault: boolean;
}

export const lanePresentations = {
  backlog: { title: "Backlog", hiddenByDefault: false },
  ready: { title: "Ready", hiddenByDefault: false },
  in_progress: { title: "In progress", hiddenByDefault: false },
  blocked: { title: "Blocked", hiddenByDefault: false },
  review: { title: "Review", hiddenByDefault: false },
  done: { title: "Done", hiddenByDefault: false },
  cancelled: { title: "Cancelled", hiddenByDefault: true },
} as const satisfies Record<WorkItemStatus, LanePresentation>;

export const laneOrder = Object.keys(lanePresentations) as WorkItemStatus[];

export const defaultLaneKeys = laneOrder.filter((status) => !lanePresentations[status].hiddenByDefault);

export interface ReadinessPresentation {
  icon: LucideIcon;
  label: string;
  tone: BadgeTone;
}

export const dependencyReadinessPresentations = {
  ready: { icon: Check, label: "Dependencies ready", tone: "positive" },
  waiting: { icon: Hourglass, label: "Waiting on dependencies", tone: "muted" },
  blocked: { icon: Ban, label: "Blocked by dependencies", tone: "warning" },
  complete: { icon: Check, label: "Dependencies complete", tone: "positive" },
} as const satisfies Record<DependencyReadiness, ReadinessPresentation>;
