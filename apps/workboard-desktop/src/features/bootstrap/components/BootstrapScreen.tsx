import { Badge, type BadgeTone } from "../../../components/ui/badge";
import type { BootstrapState } from "../../../core/contracts";
import { useBootstrap } from "../hooks/useBootstrap";

interface BootstrapPresentation {
  eyebrow: string;
  title: string;
  detail: string;
  tone: BadgeTone;
}

export const bootstrapPresentations: Record<BootstrapState, BootstrapPresentation> = {
  connecting: {
    eyebrow: "Starting",
    title: "Connecting to Workboard",
    detail: "The desktop client is locating the local Workboard daemon.",
    tone: "accent",
  },
  disconnected: {
    eyebrow: "Offline",
    title: "Workboard is unavailable",
    detail: "The desktop client will reconnect when the local daemon is available.",
    tone: "warning",
  },
  incompatible: {
    eyebrow: "Update required",
    title: "Desktop and Workboard are incompatible",
    detail: "Install compatible Desktop and daemon versions before continuing.",
    tone: "warning",
  },
  read_only: {
    eyebrow: "Read-only",
    title: "Connected without controls",
    detail: "Workboard has not advertised a compatible mutation capability.",
    tone: "accent",
  },
  resyncing: {
    eyebrow: "Refreshing",
    title: "Resynchronizing Workboard",
    detail: "An authoritative snapshot is replacing stale streamed state.",
    tone: "accent",
  },
  ready: {
    eyebrow: "Connected",
    title: "Workboard is ready",
    detail: "The secure desktop bridge is receiving ordered daemon updates.",
    tone: "positive",
  },
};

export function BootstrapScreen() {
  const { state, refusal } = useBootstrap();
  return <BootstrapStatus state={state} refusal={refusal} />;
}

export function BootstrapStatus({ state, refusal }: { state: BootstrapState; refusal?: string }) {
  const presentation = bootstrapPresentations[state];

  return (
    <main className="grid min-h-screen place-items-center bg-background px-6 text-foreground">
      <section
        aria-live="polite"
        aria-atomic="true"
        className="w-full max-w-xl rounded-3xl border border-border bg-card p-10 shadow-2xl shadow-black/20"
      >
        <Badge tone={presentation.tone} size="lg" className="mb-8 font-semibold tracking-[0.18em] uppercase">
          {presentation.eyebrow}
        </Badge>
        <h1 className="text-balance text-4xl font-semibold tracking-tight">
          {presentation.title}
        </h1>
        <p className="mt-4 max-w-md text-pretty text-base leading-7 text-muted-foreground">
          {presentation.detail}
        </p>
        {refusal !== undefined && (
          <p className="mt-4 max-w-md text-pretty break-words rounded-lg border border-warning-border p-3 text-sm">
            {refusal}
          </p>
        )}
      </section>
    </main>
  );
}
