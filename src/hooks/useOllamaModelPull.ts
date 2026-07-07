import { useCallback, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { commands, OllamaSetupError } from "@/bindings";

interface OllamaPullProgressPayload {
  model_name: string;
  status: string;
  downloaded?: number;
  total?: number;
}

export type PullOutcome =
  | { status: "present" }
  | { status: "interrupted" }
  | { status: "disk_full" }
  | { status: "no_network" }
  // Ollama rejected the name itself (e.g. no such model in its registry) —
  // retrying the same name will fail identically, so callers should prompt
  // to edit the name rather than offer a blind Retry.
  | { status: "unknown_model" }
  | { status: "other_error" };

// Substrings Ollama's daemon actually uses for "no such model" (e.g. pulling
// "this-model-does-not-exist" returns exactly {"error":"pull model manifest:
// file does not exist"} with no separate error code to match on). Used only
// to classify the outcome — the raw daemon string is never shown to the
// user; see mapSetupError below and the model.download.unknownModel copy.
const UNKNOWN_MODEL_PATTERNS = ["file does not exist", "not found", "manifest"];

function mapSetupError(error: OllamaSetupError): PullOutcome {
  switch (error.kind) {
    case "disk_full":
      return { status: "disk_full" };
    case "no_network":
      return { status: "no_network" };
    case "interrupted":
      return { status: "interrupted" };
    case "other": {
      const lower = error.detail.toLowerCase();
      const isUnknownModel = UNKNOWN_MODEL_PATTERNS.some((p) => lower.includes(p));
      return { status: isUnknownModel ? "unknown_model" : "other_error" };
    }
    default:
      return { status: "other_error" };
  }
}

/// Streams `POST /api/pull` for one model via the existing
/// `pullOllamaModel`/`ollama-pull-progress` plumbing, reporting progress
/// through `onProgress` and resolving to a typed outcome. Pure — no React
/// state — so both the setup wizard (per tier-model row, within its own
/// sequential-batch orchestration) and `useOllamaModelPull` (single-download
/// orchestration) share exactly one place that owns the progress-listener
/// and error-mapping logic.
///
/// `isStale` is checked after the listener attaches and again after the pull
/// resolves; callers use it to ignore results from a run they've since
/// abandoned (e.g. component unmounted, user started a different pull).
export async function pullModelWithProgress(
  modelName: string,
  isStale: () => boolean,
  onProgress: (percentage: number | undefined) => void,
): Promise<PullOutcome | undefined> {
  const unlisten = await listen<OllamaPullProgressPayload>(
    "ollama-pull-progress",
    (event) => {
      if (event.payload.model_name !== modelName || isStale()) return;
      const { downloaded, total } = event.payload;
      const percentage =
        downloaded !== undefined && total !== undefined && total > 0
          ? Math.min(100, Math.round((downloaded / total) * 100))
          : undefined;
      onProgress(percentage);
    },
  );

  const result = await commands.pullOllamaModel(modelName);
  unlisten();
  if (isStale()) return undefined;

  return result.status === "ok" ? { status: "present" } : mapSetupError(result.error);
}

/// Lifecycle for the cleanup mode editor's inline "Download" affordance:
/// probes first (never assumes Ollama is running), pulls directly if
/// running, or surfaces a "not_running" state the caller resolves via
/// startOllamaThenDownload() before the pull proceeds.
export type OllamaPullState =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "not_running" }
  | { status: "starting" }
  | { status: "pulling"; percentage?: number }
  | PullOutcome;

export function useOllamaModelPull() {
  const [state, setState] = useState<OllamaPullState>({ status: "idle" });
  const runIdRef = useRef(0);

  const pull = useCallback(async (modelName: string, runId: number) => {
    setState({ status: "pulling", percentage: 0 });
    const outcome = await pullModelWithProgress(
      modelName,
      () => runId !== runIdRef.current,
      (percentage) => setState({ status: "pulling", percentage }),
    );
    if (outcome) setState(outcome);
  }, []);

  const startDownload = useCallback(
    async (modelName: string) => {
      const runId = ++runIdRef.current;
      setState({ status: "checking" });

      const probe = await commands.probeOllamaSetup();
      if (runId !== runIdRef.current) return;

      if (probe.models_present.includes(modelName)) {
        setState({ status: "present" });
        return;
      }
      if (probe.availability !== "running") {
        setState({ status: "not_running" });
        return;
      }
      await pull(modelName, runId);
    },
    [pull],
  );

  /// Bound to the "Start Ollama" action shown in the not_running state.
  /// Reuses the Phase 8 install/start path.
  const startOllamaThenDownload = useCallback(
    async (modelName: string) => {
      const runId = ++runIdRef.current;
      setState({ status: "starting" });

      const result = await commands.installAndStartOllama();
      if (runId !== runIdRef.current) return;

      if (result.status === "error") {
        setState(mapSetupError(result.error));
        return;
      }
      await pull(modelName, runId);
    },
    [pull],
  );

  const retry = useCallback(
    (modelName: string) => {
      startDownload(modelName);
    },
    [startDownload],
  );

  const reset = useCallback(() => {
    runIdRef.current += 1; // invalidate any in-flight run
    setState({ status: "idle" });
  }, []);

  return { state, startDownload, startOllamaThenDownload, retry, reset };
}
