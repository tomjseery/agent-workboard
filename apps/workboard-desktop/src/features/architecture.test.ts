import { describe, expect, it } from "vitest";

const featureSources = import.meta.glob("./{workspace,hierarchy,saved-views}/**/*.{ts,tsx}", {
  eager: true,
  import: "default",
  query: "?raw",
}) as Record<string, string>;

const hierarchySources = import.meta.glob("./{workspace,hierarchy}/**/*.{ts,tsx}", {
  eager: true,
  import: "default",
  query: "?raw",
}) as Record<string, string>;

const savedViewSources = import.meta.glob("./saved-views/**/*.{ts,tsx}", {
  eager: true,
  import: "default",
  query: "?raw",
}) as Record<string, string>;

describe("feature authority boundaries", () => {
  it("keeps feature slices behind generated contracts and the daemon facade", () => {
    for (const source of Object.values(featureSources)) {
      expect(source).not.toMatch(/@tauri-apps\/api|\binvoke\s*\(|\bChannel\b|rusqlite|planning[_-]store|workboard-application/);
    }
  });

  it("keeps hierarchy server state out of the client store", () => {
    for (const source of Object.values(hierarchySources)) {
      expect(source).not.toMatch(/zustand|savedViewDraftStore/);
    }
  });

  it("keeps repository filters inside Workspace-owned view definitions", () => {
    const implementation = Object.entries(savedViewSources)
      .filter(([path]) => !path.endsWith(".test.ts"))
      .map(([, source]) => source)
      .join("\n");

    expect(implementation).toContain("workspaceId: draft.workspaceId");
    expect(implementation).toContain("repositoryIds: draft.repositoryIds");
    expect(implementation).not.toMatch(/createWorkspace|newWorkspace|databasePath|openDatabase/);
  });
});
