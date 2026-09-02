import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/utils";

export const cardVariants = cva("border border-border bg-card text-card-foreground", {
  variants: {
    size: {
      default: "rounded-2xl p-5",
      compact: "rounded-xl p-4",
      inset: "rounded-lg p-3",
      flush: "rounded-xl",
    },
  },
  defaultVariants: { size: "default" },
});

interface CardProps extends ComponentProps<"div">, VariantProps<typeof cardVariants> {
  asChild?: boolean;
}

export function Card({ className, size, asChild = false, ...props }: CardProps) {
  const Component = asChild ? Slot : "div";
  return <Component className={cn(cardVariants({ size }), className)} {...props} />;
}

export function CardTitle({ className, ...props }: ComponentProps<"h2">) {
  return <h2 className={cn("text-lg font-semibold", className)} {...props} />;
}

export function CardEyebrow({ className, ...props }: ComponentProps<"p">) {
  return <p className={cn("text-xs font-semibold tracking-[0.18em] text-primary uppercase", className)} {...props} />;
}
