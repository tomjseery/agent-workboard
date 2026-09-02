import type { ComponentProps } from "react";

import { cn } from "../../lib/utils";

export function Textarea({ className, ...props }: ComponentProps<"textarea">) {
  return (
    <textarea
      className={cn(
        "w-full rounded-lg border border-input bg-background p-3 text-sm text-foreground disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}
