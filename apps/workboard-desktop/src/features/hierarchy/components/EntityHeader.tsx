import type { ReactNode } from "react";

import { Card, CardEyebrow } from "../../../components/ui/card";
import type { RepositoryReference } from "../../../core/contracts";
import { entityPresentations } from "../model/presentation";
import type { HierarchyEntityKind } from "../types";
import { RepositoryParticipation } from "./RepositoryParticipation";

interface EntityHeaderProps {
  kind: HierarchyEntityKind;
  title: string;
  subtitle: string;
  repositories: RepositoryReference[];
  children?: ReactNode;
}

export function EntityHeader({ kind, title, subtitle, repositories, children }: EntityHeaderProps) {
  return (
    <Card asChild className="p-6">
      <header>
        <CardEyebrow>{entityPresentations[kind].eyebrow}</CardEyebrow>
        <h1 className="mt-2 text-3xl font-semibold break-words">{title}</h1>
        <p className="mt-1 text-muted-foreground">{subtitle}</p>
        <h2 className="mt-5 text-sm font-semibold">{entityPresentations[kind].participation}</h2>
        <div className="mt-2">
          <RepositoryParticipation repositories={repositories} />
        </div>
        {children}
      </header>
    </Card>
  );
}
