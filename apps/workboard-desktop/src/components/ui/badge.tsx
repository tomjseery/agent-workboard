import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/utils";

export const badgeVariants = cva("inline-flex items-center gap-1 border text-xs whitespace-nowrap", {
  variants: {
    tone: {
      neutral: "border-border text-foreground",
      muted: "border-border text-muted-foreground",
      accent: "border-accent-border text-accent-foreground",
      positive: "border-success-border text-success",
      warning: "border-warning-border text-warning",
    },
    size: {
      default: "rounded-full px-2.5 py-0.5",
      lg: "rounded-full px-3 py-1",
      tag: "rounded px-1.5 py-0.5",
    },
  },
  defaultVariants: { tone: "neutral", size: "default" },
});

export type BadgeTone = NonNullable<VariantProps<typeof badgeVariants>["tone"]>;

interface BadgeProps extends ComponentProps<"span">, VariantProps<typeof badgeVariants> {
  asChild?: boolean;
}

export function Badge({ className, tone, size, asChild = false, ...props }: BadgeProps) {
  const Component = asChild ? Slot : "span";
  return <Component className={cn(badgeVariants({ tone, size }), className)} {...props} />;
}
