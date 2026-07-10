import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Check, Loader2, RotateCcw } from "lucide-react";
import { arch, platform } from "@tauri-apps/plugin-os";
import { commands, OllamaSetupError } from "@/bindings";
import OnboardingLockup from "./OnboardingLockup";
import { Button } from "../ui/Button";
import { useSettingsStore } from "../../stores/settingsStore";
import { pullModelWithProgress } from "../../hooks/useOllamaModelPull";

interface OllamaOnboardingProps {
  onComplete: () => void;
  /// Set when embedded in the Settings resume flow (a Dialog panel) instead
  /// of rendered as a full-screen onboarding step.
  embedded?: boolean;
  /// Show the logo header. Defaults to true; the Settings resume dialog
  /// already has its own title, so it passes false.
  showLogo?: boolean;
}

// Keep in sync with src-tauri/src/settings.rs default_short_model/default_long_model.
// Apple Silicon gets the MLX-optimized tags (Metal-accelerated); every other
// platform (Windows, Linux, Intel Mac) uses the portable standard tags.
const IS_APPLE_SILICON = platform() === "macos" && arch() === "aarch64";
const SHORT_TIER_MODEL = IS_APPLE_SILICON ? "qwen3.5:2b-mlx" : "qwen3.5:2b";
const LONG_TIER_MODEL = IS_APPLE_SILICON ? "gemma4:12b-mlx" : "gemma4:12b";
const TIER_MODELS = [SHORT_TIER_MODEL, LONG_TIER_MODEL];

type OverallPhase =
  | "checking"
  | "installing"
  | "starting"
  | "pulling"
  | "configuring"
  | "ready";

type OverallErrorKind = "no_network" | "disk_full" | "other";

type ModelState =
  | "pending"
  | "pulling"
  | "present"
  | "interrupted"
  | "disk_full"
  | "skipped";

interface ModelRowState {
  state: ModelState;
  percentage?: number;
}

const initialModelRows = (): Record<string, ModelRowState> => ({
  [SHORT_TIER_MODEL]: { state: "pending" },
  [LONG_TIER_MODEL]: { state: "pending" },
});

// Minimum time a terminal (finished/error) state stays on screen before the
// wizard can auto-advance — fast paths (e.g. every model already present)
// would otherwise flash through "configuring"/"ready" too quickly to read.
// "ready" itself doesn't auto-advance at all — see the Okay button below.
const MIN_TRANSIENT_PHASE_MS = 500;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/// Runs `work`, then waits out whatever remains of `minMs` since it started.
async function withMinDuration<T>(minMs: number, work: () => Promise<T>): Promise<T> {
  const start = Date.now();
  const result = await work();
  const elapsed = Date.now() - start;
  if (elapsed < minMs) await sleep(minMs - elapsed);
  return result;
}

/// First-run Ollama cleanup setup: detects/installs/starts Ollama, pulls the
/// two adaptive-cleanup tier models with progress, then auto-configures
/// cleanup to use them. Every failure branch offers Retry/Skip-this-model;
/// "Skip for now" is always visible and never blocks reaching onComplete().
const OllamaOnboarding: React.FC<OllamaOnboardingProps> = ({
  onComplete,
  embedded = false,
  showLogo = true,
}) => {
  const { t } = useTranslation();
  const refreshSettings = useSettingsStore((state) => state.refreshSettings);
  const [phase, setPhase] = useState<OverallPhase>("checking");
  const [overallError, setOverallError] = useState<OverallErrorKind | null>(
    null,
  );
  const [installProgress, setInstallProgress] = useState<number | undefined>(
    undefined,
  );
  const [modelRows, setModelRows] =
    useState<Record<string, ModelRowState>>(initialModelRows);
  const runIdRef = useRef(0);
  const modelRowsRef = useRef(modelRows);
  modelRowsRef.current = modelRows;
  // Guards autoConfigure() against firing more than once per run — the
  // terminal-state effect below can re-render while still in a terminal
  // state (e.g. while finish()'s setTimeout is pending).
  const autoConfiguredRunRef = useRef(0);
  // When every model is already present, pullMissingModels sets phase to
  // "pulling" and the terminal-state effect can fire autoConfigure() on the
  // very next render — the checkmark rows would flash by unread. Tracks when
  // "pulling" started so autoConfigure can hold it for a minimum beat first.
  const pullingEnteredAtRef = useRef(0);

  // Terminal-state dwell before closing. `displayMs` should already reflect
  // however long the state shown just before calling this has been visible
  // (callers use withMinDuration for that); this only adds the closing beat.
  const finish = useCallback(
    (displayMs: number = MIN_TRANSIENT_PHASE_MS) => {
      setTimeout(() => onComplete(), displayMs);
    },
    [onComplete],
  );

  const errorKindFor = (error: OllamaSetupError): OverallErrorKind => {
    if (error.kind === "no_network") return "no_network";
    if (error.kind === "disk_full") return "disk_full";
    return "other";
  };

  // Shares the pull/progress/error-mapping primitive with the mode editor's
  // inline download (useOllamaModelPull); this row keeps its own simpler
  // present/interrupted/disk_full states since Ollama's running-state is
  // already resolved once, wizard-wide, before any row starts pulling.
  const pullOneModel = useCallback(
    async (model: string, runId: number) => {
      setModelRows((prev) => ({
        ...prev,
        [model]: { state: "pulling", percentage: 0 },
      }));

      const outcome = await pullModelWithProgress(
        model,
        () => runId !== runIdRef.current,
        (percentage) =>
          setModelRows((prev) => ({
            ...prev,
            [model]: { state: "pulling", percentage },
          })),
      );
      if (!outcome) return; // stale run

      const nextState: ModelState =
        outcome.status === "present"
          ? "present"
          : outcome.status === "disk_full"
            ? "disk_full"
            : "interrupted";
      setModelRows((prev) => ({ ...prev, [model]: { state: nextState } }));
    },
    [],
  );

  const autoConfigure = useCallback(async () => {
    if (autoConfiguredRunRef.current === runIdRef.current) return;
    autoConfiguredRunRef.current = runIdRef.current;

    // Hold the "pulling" view (which may just be a checkmark list, if every
    // model was already present) for a minimum beat before moving on —
    // otherwise the terminal-state effect can call this on the very next
    // render and the checkmarks flash by unread.
    const pullingElapsed = Date.now() - pullingEnteredAtRef.current;
    if (pullingElapsed < MIN_TRANSIENT_PHASE_MS) {
      await sleep(MIN_TRANSIENT_PHASE_MS - pullingElapsed);
    }

    const present = TIER_MODELS.filter(
      (m) => modelRowsRef.current[m]?.state === "present",
    );
    if (present.length === 0) {
      // Nothing pulled successfully: leave post-process settings untouched
      // (still defaults to disabled/openai) so the Settings resume card
      // keeps offering to try again — this is not a "Completed" outcome.
      finish();
      return;
    }
    setPhase("configuring");
    // Prefer the long-tier (thorough) model as the single-model fallback
    // used outside adaptive routing / mode overrides.
    const configuredModel = present.includes(LONG_TIER_MODEL)
      ? LONG_TIER_MODEL
      : present[0];
    // withMinDuration keeps "configuring" visible for a moment even when
    // every model was already present and this resolves almost instantly.
    await withMinDuration(MIN_TRANSIENT_PHASE_MS, async () => {
      await commands.changePostProcessEnabledSetting(true);
      await commands.setPostProcessProvider("custom");
      await commands.changePostProcessModelSetting("custom", configuredModel);
      await commands.setOllamaSetupStatus("completed");
      // These commands mutate settings directly (not via the settingsStore's
      // update* wrappers), so the cached store never learns about the change
      // on its own — without this, "Set up Ollama" keeps showing after a
      // successful run because getSetting("ollama_setup_status") still
      // reads the stale cached value.
      await refreshSettings();
    });
    // Wait for the user to confirm via the "Okay" button rather than
    // auto-closing — the "ready" screen is the one thing worth letting
    // people actually read before it goes away.
    setPhase("ready");
  }, [refreshSettings]);

  // Sequential per-model pulls; the terminal-state effect below fires
  // autoConfigure() once every row lands in a final, non-retryable state —
  // whether that's immediately (nothing to pull), after both pulls succeed,
  // or after the user resolves a retryable failure via Retry/Skip.
  const pullMissingModels = useCallback(
    async (alreadyPresent: string[], runId: number) => {
      setModelRows((prev) => {
        const next = { ...prev };
        for (const model of alreadyPresent) {
          next[model] = { state: "present" };
        }
        return next;
      });
      pullingEnteredAtRef.current = Date.now();
      setPhase("pulling");
      for (const model of TIER_MODELS) {
        if (runId !== runIdRef.current) return;
        if (alreadyPresent.includes(model)) continue;
        await pullOneModel(model, runId);
      }
    },
    [pullOneModel],
  );

  const runSetup = useCallback(async () => {
    const runId = ++runIdRef.current;
    setOverallError(null);
    setModelRows(initialModelRows());
    setInstallProgress(undefined);
    setPhase("checking");

    const probe = await commands.probeOllamaSetup();
    if (runId !== runIdRef.current) return;

    if (probe.availability === "running") {
      await pullMissingModels(probe.models_present, runId);
      return;
    }

    setPhase(probe.availability === "installed_not_running" ? "starting" : "installing");
    const result = await commands.installAndStartOllama();
    if (runId !== runIdRef.current) return;

    if (result.status === "error") {
      setOverallError(errorKindFor(result.error));
      return;
    }

    // Re-probe now that the service should be running to get an accurate
    // already-pulled model list before starting any pulls.
    const rechecked = await commands.probeOllamaSetup();
    if (runId !== runIdRef.current) return;
    await pullMissingModels(rechecked.models_present, runId);
  }, [pullMissingModels]);

  // Intentionally run once on mount only — re-running on every dependency
  // change would restart the wizard mid-flow.
  useEffect(() => {
    runSetup();
  }, []);

  // Reflect installer download progress (macOS zip / Windows exe) into the
  // "installing" status line. The event only fires while a download is in
  // flight, so a persistent listener is fine.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<{ downloaded: number; total: number | null }>(
      "ollama-install-progress",
      (event) => {
        const { downloaded, total } = event.payload;
        setInstallProgress(
          total && total > 0
            ? Math.min(100, Math.round((downloaded / total) * 100))
            : undefined,
        );
      },
    ).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const handleSkip = async () => {
    runIdRef.current += 1; // invalidate any in-flight run
    try {
      await commands.setOllamaSetupStatus("skipped");
      await refreshSettings();
    } catch (e) {
      console.warn("Failed to record Ollama setup skip:", e);
    }
    onComplete();
  };

  const handleRetryModel = (model: string) => {
    // The terminal-state effect below re-checks once this pull settles and
    // calls autoConfigure() itself if every row has reached a final state.
    pullOneModel(model, runIdRef.current);
  };

  const handleSkipModel = (model: string) => {
    setModelRows((prev) => {
      const next = { ...prev, [model]: { state: "skipped" as ModelState } };
      return next;
    });
  };

  // Once every model row reaches a terminal state, auto-configure/advance.
  useEffect(() => {
    if (phase !== "pulling") return;
    const terminal = TIER_MODELS.every((m) =>
      ["present", "interrupted", "disk_full", "skipped"].includes(
        modelRows[m]?.state,
      ),
    );
    const anyRetryable = TIER_MODELS.some((m) =>
      ["interrupted", "disk_full"].includes(modelRows[m]?.state),
    );
    if (terminal && !anyRetryable) {
      autoConfigure();
    }
  }, [phase, modelRows, autoConfigure]);

  const statusLabel = (() => {
    switch (phase) {
      case "checking":
        return t("onboarding.ollama.checking");
      case "installing":
        return installProgress !== undefined
          ? t("onboarding.ollama.downloading", { percentage: installProgress })
          : t("onboarding.ollama.installing");
      case "starting":
        return t("onboarding.ollama.starting");
      case "configuring":
        return t("onboarding.ollama.configuring");
      default:
        return null;
    }
  })();

  const modelLabel = (model: string) =>
    model === SHORT_TIER_MODEL
      ? t("onboarding.ollama.model.short")
      : t("onboarding.ollama.model.long");

  return (
    <div
      className={
        embedded
          ? "w-full flex flex-col gap-6 items-center"
          : "h-screen w-screen flex flex-col p-6 gap-6 items-center justify-center"
      }
    >
      {showLogo && (
        <div className="flex flex-col items-center gap-2">
          <OnboardingLockup />
        </div>
      )}

      <div className="max-w-md w-full flex flex-col items-center gap-4">
        {!embedded && (
          <div className="text-center mb-2">
            <h2 className="text-xl font-semibold text-text mb-2">
              {t("onboarding.ollama.title")}
            </h2>
            <p className="text-text/70">
              {t("onboarding.ollama.description")}
            </p>
          </div>
        )}

        {overallError && (
          <div className="w-full p-4 rounded-lg bg-red-500/10 border border-red-500/30">
            <p className="text-sm text-text mb-3">
              {t(`onboarding.ollama.errors.${overallError}`)}
            </p>
            <div className="flex gap-2">
              <Button onClick={runSetup} variant="primary" size="sm">
                <RotateCcw className="w-3.5 h-3.5 mr-1.5" />
                {t("onboarding.ollama.retry")}
              </Button>
            </div>
          </div>
        )}

        {!overallError && statusLabel && (
          <div className="flex items-center gap-2 text-text/70 text-sm">
            <Loader2 className="w-4 h-4 animate-spin" />
            {statusLabel}
          </div>
        )}

        {!overallError && phase === "pulling" && (
          <div className="w-full flex flex-col gap-3">
            {TIER_MODELS.map((model) => {
              const row = modelRows[model] ?? { state: "pending" };
              return (
                <div
                  key={model}
                  className="w-full p-3 rounded-lg bg-white/5 border border-mid-gray/20"
                >
                  <div className="flex items-center justify-between mb-1">
                    <span className="text-sm font-medium text-text">
                      {modelLabel(model)}
                    </span>
                    {row.state === "present" && (
                      <span className="flex items-center gap-1 text-emerald-400 text-xs">
                        <Check className="w-3.5 h-3.5" />
                        {t("onboarding.ollama.model.present")}
                      </span>
                    )}
                    {row.state === "skipped" && (
                      <span className="text-text/50 text-xs">
                        {t("onboarding.ollama.model.skipped")}
                      </span>
                    )}
                  </div>

                  {row.state === "pulling" && (
                    <div className="w-full h-1.5 bg-mid-gray/20 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-logo-primary rounded-full transition-all duration-300"
                        style={{ width: `${row.percentage ?? 0}%` }}
                      />
                    </div>
                  )}

                  {(row.state === "interrupted" || row.state === "disk_full") && (
                    <div className="flex flex-col gap-2">
                      <p className="text-xs text-text/60">
                        {t(`onboarding.ollama.model.${row.state}`)}
                      </p>
                      <div className="flex gap-2">
                        <Button
                          onClick={() => handleRetryModel(model)}
                          variant="secondary"
                          size="sm"
                        >
                          {t("onboarding.ollama.model.retryThis")}
                        </Button>
                        <Button
                          onClick={() => handleSkipModel(model)}
                          variant="secondary"
                          size="sm"
                        >
                          {t("onboarding.ollama.model.skipThis")}
                        </Button>
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}

        {phase === "ready" && (
          <div className="flex flex-col items-center gap-3">
            <div className="p-4 rounded-full bg-emerald-500/20">
              <Check className="w-10 h-10 text-emerald-400" />
            </div>
            <p className="text-text/70 text-sm">
              {t("onboarding.ollama.ready")}
            </p>
            <Button onClick={onComplete} variant="primary" size="sm">
              {t("onboarding.ollama.okay")}
            </Button>
          </div>
        )}
      </div>

      <button
        onClick={handleSkip}
        className="text-sm text-text/50 hover:text-text/70 transition-colors underline"
      >
        {t("onboarding.ollama.skip")}
      </button>
    </div>
  );
};

export default OllamaOnboarding;
