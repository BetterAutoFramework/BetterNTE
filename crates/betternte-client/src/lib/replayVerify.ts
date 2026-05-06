import { invokeAction } from "./stores/helpers";

/** Standard session layout (`replay_expect.json`, `timeline.jsonl`, …) under `replay.artifact_root / session_name`. */
export async function replayVerifySession(sessionName: string): Promise<string> {
  return invokeAction<string>("replay_verify_session", {
    session_name: sessionName,
  });
}

/**
 * Paths relative to configured `replay.artifact_root`. Files must exist (same as CLI).
 * Example: `expect_relative`: `"mysession/replay_expect.json"`.
 */
export async function replayVerifyArtifacts(
  expectRelative: string,
  timelineRelative: string,
  manifestRelative?: string | null,
): Promise<string> {
  return invokeAction<string>("replay_verify_artifacts", {
    expect_relative: expectRelative,
    timeline_relative: timelineRelative,
    manifest_relative: manifestRelative ?? null,
  });
}
