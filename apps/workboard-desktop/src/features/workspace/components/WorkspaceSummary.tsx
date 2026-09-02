import { Card, CardTitle } from "../../../components/ui/card";
import type { WorkspaceSummary as WorkspaceSummaryContract } from "../../../core/contracts";

interface WorkspaceSummaryProps {
  summary: WorkspaceSummaryContract;
}

export function WorkspaceSummary({ summary }: WorkspaceSummaryProps) {
  const counts = [
    ["Repositories", summary.repositoryCount],
    ["Epics", summary.epicCount],
    ["Features", summary.featureCount],
    ["Work items", summary.workItemCount],
  ] as const;

  return (
    <Card asChild>
      <section aria-labelledby="workspace-summary-title">
      <CardTitle id="workspace-summary-title">Workspace overview</CardTitle>
      <dl className="mt-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
        {counts.map(([label, value]) => (
          <div key={label} className="rounded-xl bg-muted p-4">
            <dt className="text-sm text-muted-foreground">{label}</dt>
            <dd className="mt-1 text-2xl font-semibold">{value}</dd>
          </div>
        ))}
      </dl>
    </section>
    </Card>
  );
}
