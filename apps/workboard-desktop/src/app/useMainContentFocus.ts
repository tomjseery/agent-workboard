import { useRouterState } from "@tanstack/react-router";
import { useEffect } from "react";

export const mainContentId = "main-content";

export function useMainContentFocus() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  useEffect(() => {
    requestAnimationFrame(() => document.getElementById(mainContentId)?.focus());
  }, [pathname]);
}
