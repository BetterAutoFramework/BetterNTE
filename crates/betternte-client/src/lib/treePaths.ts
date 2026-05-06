import type { ScriptInfo } from "@/lib/types";

function normalizePath(path?: string): string[] {
  if (!path) return [];
  return path.replaceAll("\\", "/").split("/").filter(Boolean);
}

function buildScriptRelativePath(dir: string | undefined, marker: "scripts" | "triggers"): string {
  const segments = normalizePath(dir);
  const markerIdx = segments.findIndex((segment) => segment === marker);
  if (markerIdx >= 0 && markerIdx + 1 < segments.length) {
    // `.../scripts/<parent...>/<script_dir>`:
    // keep only parent folders, script_dir itself is rendered as the leaf item label.
    const scriptRelative = segments.slice(markerIdx + 1);
    const parentSegments = scriptRelative.slice(0, -1);
    return parentSegments.join("/");
  }
  return "";
}

export function buildScriptTreePath(script: ScriptInfo): string {
  const source = script.source?.trim() || "未分类";
  const marker = script.type === "trigger" ? "triggers" : "scripts";
  const relative = buildScriptRelativePath(script.dir, marker);
  return relative ? `${source}/${relative}` : source;
}

