import { RouterProvider } from "@tanstack/react-router";

import { BootstrapStatus } from "../features/bootstrap/components/BootstrapScreen";
import { useBootstrap } from "../features/bootstrap/hooks/useBootstrap";
import { router } from "../routes/router";

export function App() {
  const bootstrap = useBootstrap();
  if ((bootstrap.state !== "ready" && bootstrap.state !== "read_only") || bootstrap.workspaceId === undefined) {
    return <BootstrapStatus state={bootstrap.state} refusal={bootstrap.refusal} />;
  }
  return <RouterProvider router={router} context={{ workspaceId: bootstrap.workspaceId }} />;
}
