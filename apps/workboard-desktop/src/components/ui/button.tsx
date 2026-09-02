import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";

import { cn } from "../../lib/utils";

export const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 rounded-lg font-medium whitespace-nowrap disabled:pointer-events-none disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-ring [&_svg]:pointer-events-none [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        outline: "border border-border bg-transparent hover:bg-muted",
        solid: "bg-primary text-primary-foreground hover:opacity-90",
        ghost: "hover:bg-muted",
        link: "text-primary underline underline-offset-2",
      },
      size: {
        default: "px-3 py-2 text-sm",
        lg: "px-4 py-2",
        sm: "px-2.5 py-1 text-xs",
        icon: "size-5 rounded",
      },
    },
    defaultVariants: { variant: "outline", size: "default" },
  },
);

interface ButtonProps extends ComponentProps<"button">, VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

export function Button({ className, variant, size, asChild = false, ...props }: ButtonProps) {
  const Component = asChild ? Slot : "button";
  return <Component className={cn(buttonVariants({ variant, size }), className)} {...props} />;
}
