import type { BootstrapState } from "../../../core/generated";
import { useBootstrap } from "../hooks/useBootstrap";

interface BootstrapPresentation {
  eyebrow: string;
  title: string;
  detail: string;
  tone: "calm" | "warning" | "success";
}

export const bootstrapPresentations: Record<BootstrapState, BootstrapPresentation> = {
  connecting: {
    eyebrow: "Starting",
    title: "Connecting to Workboard",
    detail: "The desktop client is locating the local Workboard daemon.",
    tone: "calm",
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
    tone: "calm",
  },
  resyncing: {
    eyebrow: "Refreshing",
    title: "Resynchronizing Workboard",
    detail: "An authoritative snapshot is replacing stale streamed state.",
    tone: "calm",
  },
  ready: {
    eyebrow: "Connected",
    title: "Workboard is ready",
    detail: "The secure desktop bridge is receiving ordered daemon updates.",
    tone: "success",
  },
};

const toneClasses: Record<BootstrapPresentation["tone"], string> = {
  calm: "text-[var(--accent)] border-[var(--accent-muted)]",
  warning: "text-[var(--warning)] border-[var(--warning-muted)]",
  success: "text-[var(--success)] border-[var(--success-muted)]",
};

export function BootstrapScreen() {
  const { state } = useBootstrap();
  const presentation = bootstrapPresentations[state];

  return (
    <main className="grid min-h-screen place-items-center bg-[var(--canvas)] px-6 text-[var(--text)]">
      <section
        aria-live="polite"
        aria-atomic="true"
        className="w-full max-w-xl rounded-3xl border border-[var(--border)] bg-[var(--surface)] p-10 shadow-2xl shadow-black/20"
      >
        <div
          className={`mb-8 inline-flex rounded-full border px-3 py-1 text-xs font-semibold uppercase tracking-[0.18em] ${toneClasses[presentation.tone]}`}
        >
          {presentation.eyebrow}
        </div>
        <h1 className="text-balance text-4xl font-semibold tracking-tight">
          {presentation.title}
        </h1>
        <p className="mt-4 max-w-md text-pretty text-base leading-7 text-[var(--muted-text)]">
          {presentation.detail}
        </p>
      </section>
    </main>
  );
}
