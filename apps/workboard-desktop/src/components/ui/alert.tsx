import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/utils";

export const alertVariants = cva("border", {
  variants: {
    tone: {
      warning: "border-warning-border",
      neutral: "border-border bg-card",
    },
    size: {
      default: "rounded-lg p-3",
      lg: "rounded-xl p-4",
    },
  },
  defaultVariants: { tone: "warning", size: "default" },
});

interface AlertProps extends ComponentProps<"div">, VariantProps<typeof alertVariants> {}

export function Alert({ className, tone, size, role = "alert", ...props }: AlertProps) {
  return <div role={role} className={cn(alertVariants({ tone, size }), className)} {...props} />;
}
