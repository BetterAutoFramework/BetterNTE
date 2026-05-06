import type { EngineConfig } from "./types";

export function findScriptShortcut(config: EngineConfig, scriptName: string): string {
  for (const [sc, n] of Object.entries(config.hotkey_triggers.scripts)) {
    if (n === scriptName) return sc;
  }
  return "";
}

export function findTaskGroupShortcut(config: EngineConfig, uuid: string): string {
  for (const [sc, id] of Object.entries(config.hotkey_triggers.task_groups)) {
    if (id === uuid) return sc;
  }
  return "";
}

/** Remove any mapping for this script, optionally set a new global shortcut (clears conflicting shortcut from both maps). */
export function upsertScriptHotkey(
  config: EngineConfig,
  scriptName: string,
  shortcutTrimmed: string
): EngineConfig {
  const scripts = { ...config.hotkey_triggers.scripts };
  const task_groups = { ...config.hotkey_triggers.task_groups };
  for (const [sc, n] of Object.entries(scripts)) {
    if (n === scriptName) delete scripts[sc];
  }
  const sc = shortcutTrimmed.trim();
  if (!sc) {
    return {
      ...config,
      hotkey_triggers: { scripts, task_groups },
    };
  }
  delete scripts[sc];
  delete task_groups[sc];
  scripts[sc] = scriptName;
  return {
    ...config,
    hotkey_triggers: { scripts, task_groups },
  };
}

/** Remove any mapping for this task group uuid, optionally set a new shortcut. */
export function upsertTaskGroupHotkey(
  config: EngineConfig,
  uuid: string,
  shortcutTrimmed: string
): EngineConfig {
  const scripts = { ...config.hotkey_triggers.scripts };
  const task_groups = { ...config.hotkey_triggers.task_groups };
  for (const [sc, id] of Object.entries(task_groups)) {
    if (id === uuid) delete task_groups[sc];
  }
  const sc = shortcutTrimmed.trim();
  if (!sc) {
    return {
      ...config,
      hotkey_triggers: { scripts, task_groups },
    };
  }
  delete scripts[sc];
  delete task_groups[sc];
  task_groups[sc] = uuid;
  return {
    ...config,
    hotkey_triggers: { scripts, task_groups },
  };
}
