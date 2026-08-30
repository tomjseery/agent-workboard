import type { WorkspaceSummary as WorkspaceSummaryContract } from "../../../core/generated";

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
    <section aria-labelledby="workspace-summary-title" className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5">
      <h2 id="workspace-summary-title" className="text-lg font-semibold">Workspace overview</h2>
      <dl className="mt-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
        {counts.map(([label, value]) => (
          <div key={label} className="rounded-xl bg-[var(--surface-muted)] p-4">
            <dt className="text-sm text-[var(--muted-text)]">{label}</dt>
            <dd className="mt-1 text-2xl font-semibold">{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
