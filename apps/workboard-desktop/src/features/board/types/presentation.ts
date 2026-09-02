import type { DependencyReadiness, WorkItemStatus } from "../../../core/generated";

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
  symbol: string;
  label: string;
  tone: "neutral" | "positive" | "warning";
}

export const dependencyReadinessPresentations = {
  ready: { symbol: "✓", label: "Dependencies ready", tone: "positive" },
  waiting: { symbol: "⧗", label: "Waiting on dependencies", tone: "neutral" },
  blocked: { symbol: "⊘", label: "Blocked by dependencies", tone: "warning" },
  complete: { symbol: "✓", label: "Dependencies complete", tone: "positive" },
} as const satisfies Record<DependencyReadiness, ReadinessPresentation>;

export const readinessToneClasses: Record<ReadinessPresentation["tone"], string> = {
  neutral: "border-[var(--border)] text-[var(--muted-text)]",
  positive: "border-[var(--success-muted)] text-[var(--success)]",
  warning: "border-[var(--warning-muted)] text-[var(--warning)]",
};
