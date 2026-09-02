import * as CollapsiblePrimitive from "@radix-ui/react-collapsible";
import { ChevronRight } from "lucide-react";
import type { ComponentProps } from "react";

import { cn } from "../../lib/utils";

export const Collapsible = CollapsiblePrimitive.Root;
export const CollapsibleContent = CollapsiblePrimitive.Content;

export function CollapsibleTrigger({ className, ...props }: ComponentProps<typeof CollapsiblePrimitive.Trigger>) {
  return (
    <CollapsiblePrimitive.Trigger
      className={cn("group flex w-5 shrink-0 items-center justify-center rounded text-muted-foreground", className)}
      {...props}
    >
      <ChevronRight className="size-4 transition-transform group-data-[state=open]:rotate-90" aria-hidden />
    </CollapsiblePrimitive.Trigger>
  );
}
