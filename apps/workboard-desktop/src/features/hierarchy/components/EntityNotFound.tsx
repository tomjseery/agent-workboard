import { entityPresentations } from "../model/presentation";
import type { HierarchyEntityKind } from "../types";

export function EntityNotFound({ kind }: { kind: HierarchyEntityKind }) {
  return (
    <section aria-labelledby="missing-title">
      <h1 id="missing-title" className="text-2xl font-semibold">{entityPresentations[kind].eyebrow} not found</h1>
      <p className="mt-2">This deep link no longer resolves in the current Workspace hierarchy.</p>
    </section>
  );
}
