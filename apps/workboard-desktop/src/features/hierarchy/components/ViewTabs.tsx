import type { ReactNode } from "react";

export function ViewTabs({ label, children }: { label: string; children: ReactNode }) {
  return <nav aria-label={label}><ul className="flex flex-wrap gap-2 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-2">{children}</ul></nav>;
}

export const tabClasses = "inline-block rounded-lg px-3 py-2 text-sm";
export const tabActiveClasses = "inline-block rounded-lg bg-[var(--accent-surface)] px-3 py-2 text-sm font-semibold text-[var(--accent)]";
