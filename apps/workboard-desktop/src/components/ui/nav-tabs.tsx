import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps, ReactNode } from "react";

import { cn } from "../../lib/utils";

export const navTabVariants = cva("inline-block rounded-lg text-sm", {
  variants: {
    active: {
      true: "bg-accent font-semibold text-accent-foreground",
      false: "",
    },
    size: {
      default: "px-3 py-2",
      compact: "px-3 py-1.5",
    },
  },
  defaultVariants: { active: false, size: "default" },
});

export function navTabProps(size?: VariantProps<typeof navTabVariants>["size"]) {
  return {
    className: navTabVariants({ size }),
    activeProps: { className: navTabVariants({ size, active: true }), "aria-current": "page" as const },
  };
}

interface NavTabsProps extends Omit<ComponentProps<"nav">, "children"> {
  label: string;
  children: ReactNode;
}

export function NavTabs({ label, className, children, ...props }: NavTabsProps) {
  return (
    <nav aria-label={label} {...props}>
      <ul className={cn("flex flex-wrap gap-2 rounded-xl border border-border bg-card p-2", className)}>{children}</ul>
    </nav>
  );
}
