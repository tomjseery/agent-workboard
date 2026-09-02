import type { ComponentProps } from "react";

import { cn } from "../../lib/utils";

export function Select({ className, ...props }: ComponentProps<"select">) {
  return (
    <select
      className={cn(
        "rounded-lg border border-input bg-background px-3 py-2 text-sm text-foreground disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}
