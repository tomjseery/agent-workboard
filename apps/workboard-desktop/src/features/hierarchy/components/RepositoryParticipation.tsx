import type { RepositoryId } from "../../../core/generated";
import type { HierarchyModel } from "../types/hierarchy";

interface RepositoryParticipationProps {
  hierarchy: HierarchyModel;
  repositoryIds: RepositoryId[];
}

export function RepositoryParticipation({ hierarchy, repositoryIds }: RepositoryParticipationProps) {
  if (repositoryIds.length === 0) return <span className="text-sm text-[var(--muted-text)]">No repository participation recorded</span>;
  return (
    <ul aria-label="Participating repositories" className="flex flex-wrap gap-2">
      {repositoryIds.map((repositoryId) => {
        const repository = hierarchy.repositories.find((candidate) => candidate.id === repositoryId);
        return <li key={repositoryId} className="rounded-full border border-[var(--border)] px-2.5 py-1 text-xs">{repository?.title ?? "Missing repository"}</li>;
      })}
    </ul>
  );
}
