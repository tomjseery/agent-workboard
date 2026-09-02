import { describe, expect, it } from "vitest";

const featureSources = import.meta.glob("./{workspace,hierarchy,navigation,saved-views,board,repository,checkout,session,proposal,work-item}/**/*.{ts,tsx}", {
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

const boardStoreSources = import.meta.glob("./board/store/**/*.{ts,tsx}", { eager: true, import: "default", query: "?raw" }) as Record<string, string>;
const operationalSources = import.meta.glob("./{repository,checkout,session}/**/*.{ts,tsx}", { eager: true, import: "default", query: "?raw" }) as Record<string, string>;
const proposalSources = import.meta.glob("./proposal/**/*.{ts,tsx}", { eager: true, import: "default", query: "?raw" }) as Record<string, string>;
const workItemSources = import.meta.glob("./work-item/**/*.{ts,tsx}", { eager: true, import: "default", query: "?raw" }) as Record<string, string>;
const navigationSources = import.meta.glob("./navigation/**/*.{ts,tsx}", { eager: true, import: "default", query: "?raw" }) as Record<string, string>;

describe("feature authority boundaries", () => {
  it("keeps feature slices behind generated contracts and the daemon facade", () => {
    for (const source of Object.values(featureSources)) {
      expect(source).not.toMatch(/@tauri-apps\/api|\binvoke\s*\(|\bChannel\b|rusqlite|planning[_-]store|workboard-application/);
    }
  });

  it("keeps native repository session and recovery evidence behind the Rust boundary", () => {
    for (const source of Object.values(operationalSources)) {
      expect(source).not.toMatch(/@tauri-apps\/api|\binvoke\s*\(|\bChannel\b|window\.__TAURI__|node:fs|child_process|\bDeno\b|\bBun\b|\bWebSocket\b|\bfetch\s*\(|process\.(?:cwd|env|pid)|transcript|credential|accessToken|refreshToken/);
      expect(source).not.toMatch(/zustand|createStore|useStore/);
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
  it("keeps daemon board dependency attention session and workflow facts out of Zustand", () => {
    const implementation = Object.values(boardStoreSources).join("\\n");
    expect(implementation).not.toMatch(/BoardCardProjection|AttentionEntryProjection|DependencyReadiness|SessionSummary|AvailableAction/);
    expect(implementation).toContain("selectedWorkItemId");
    expect(implementation).toContain("focusedWorkItemId");
  });

  it("keeps navigation structural and its expansion state the only client-owned fact", () => {
    const navigation = (segment: string) => Object.entries(navigationSources).filter(([path]) => path.includes(segment)).map(([, source]) => source).join("\n");

    expect(navigation("/store/")).toContain("overrides");
    expect(navigation("/store/")).not.toMatch(/WorkspaceHierarchy|HierarchyEpic|HierarchyFeature|HierarchyWorkItem|BoardCardProjection|AvailableAction/);
    expect(navigation("/model/")).not.toMatch(/zustand|useQuery|daemon/);

    for (const [path, source] of Object.entries(navigationSources)) {
      if (path.includes(".test.")) continue;
      expect(source).not.toMatch(/@tauri-apps\/api|\binvoke\s*\(|\bChannel\b|node:fs|child_process|process\.|dangerouslySetInnerHTML/);
    }
  });

  it("keeps proposal authority and unsafe native surfaces outside React", () => {
    for (const [path, source] of Object.entries(proposalSources)) {
      if (path.includes(".test.")) continue;
      expect(source).not.toMatch(/zustand|createStore|useStore|@tauri-apps\/api|\binvoke\s*\(|\bChannel\b|node:fs|child_process|process\.|transcript|credential|planning[_-]store|publication[_-](?:store|retry)|workboard\s+workflow|dangerouslySetInnerHTML|javascript:/i);
    }
  });

  it("keeps Work-item authority in generated read contracts with no checkpoint escape hatch", () => {
    const implementation = Object.entries(workItemSources).filter(([path]) => !path.includes(".test.")).map(([, source]) => source).join("\n");
    expect(implementation).not.toMatch(/zustand|createStore|useStore|@tauri-apps\/api|\binvoke\s*\(|\bChannel\b|node:fs|child_process|process\.|transcript|credential|planning[_-]store|publication|checkpoint[_-](?:store|storage|write)|workboard\s+workflow|markdown|\.md\b|\bfetch\s*\(|WebSocket|git\s|shell|commandLine/i);
    expect(implementation).not.toMatch(/checkpointWorkItem|dangerouslySetInnerHTML/);
    expect(implementation).toContain("availableActions");

    const commands = [...implementation.matchAll(/type:\s*"([a-z_]+)"\s*,\s*value:/g)].map(([, code]) => code);
    expect([...new Set(commands)].sort()).toEqual(["resume_session", "start_session"]);
  });
});
