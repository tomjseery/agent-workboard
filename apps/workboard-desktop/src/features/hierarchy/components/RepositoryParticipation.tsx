import { Badge } from "../../../components/ui/badge";
import type { RepositoryReference } from "../../../core/contracts";

export function RepositoryParticipation({ repositories }: { repositories: RepositoryReference[] }) {
  if (repositories.length === 0) return <span className="text-sm text-muted-foreground">No repository participation recorded</span>;
  return (
    <ul aria-label="Participating repositories" className="flex flex-wrap gap-2">
      {repositories.map((repository) => (
        <li key={repository.id}>
          <Badge className="py-1">{repository.title}</Badge>
        </li>
      ))}
    </ul>
  );
}
