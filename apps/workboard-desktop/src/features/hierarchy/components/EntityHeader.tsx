import type { ReactNode } from "react";

import type { RepositoryId } from "../../../core/generated";
import type { HierarchyEntityKind, HierarchyModel } from "../types/hierarchy";
import { entityPresentations } from "../types/presentation";
import { RepositoryParticipation } from "./RepositoryParticipation";

interface EntityHeaderProps {
  kind: HierarchyEntityKind;
  title: string;
  subtitle: string;
  hierarchy: HierarchyModel;
  repositoryIds: RepositoryId[];
  children?: ReactNode;
}

export function EntityHeader({ kind, title, subtitle, hierarchy, repositoryIds, children }: EntityHeaderProps) {
  return (
    <header className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-6">
      <p className="text-xs font-semibold uppercase tracking-[0.18em] text-[var(--accent)]">{entityPresentations[kind].eyebrow}</p>
      <h1 className="mt-2 text-3xl font-semibold break-words">{title}</h1>
      <p className="mt-1 text-[var(--muted-text)]">{subtitle}</p>
      <h2 className="mt-5 text-sm font-semibold">{entityPresentations[kind].participation}</h2>
      <div className="mt-2"><RepositoryParticipation hierarchy={hierarchy} repositoryIds={repositoryIds} /></div>
      {children}
    </header>
  );
}
