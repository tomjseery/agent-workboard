import type { ComponentProps } from "react";

import { cn } from "../../lib/utils";

export function Radio({ className, ...props }: Omit<ComponentProps<"input">, "type">) {
  return <input type="radio" className={cn("size-4 accent-primary disabled:opacity-50", className)} {...props} />;
}
