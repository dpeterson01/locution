import { useEffect, useState, useRef } from "react";
import { toast, Toaster } from "sonner";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { platform } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  checkMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import {
  DiagnosticEvent,
  FailureCategory,
  ModelStateEvent,
  RecordingErrorEvent,
} from "./lib/types/events";
import "./App.css";
import AccessibilityPermissions from "./components/AccessibilityPermissions";
import Footer from "./components/footer";
import Onboarding, {
  AccessibilityOnboarding,
  OllamaOnboarding,
  DictionaryOnboarding,
  UsageOnboarding,
} from "./components/onboarding";
import { Sidebar, SidebarSection, SECTIONS_CONFIG } from "./components/Sidebar";
import { WhatsNewGate } from "./components/whats-new";
import { useSettings } from "./hooks/useSettings";
import { useSettingsStore } from "./stores/settingsStore";
import { commands } from "@/bindings";
import { getLanguageDirection, initializeRTL } from "@/lib/utils/rtl";

type OnboardingStep =
  | "accessibility"
  | "model"
  | "ollama"
  | "dictionary"
  | "usage"
  | "done";

const renderSettingsContent = (section: SidebarSection) => {
  const ActiveComponent =
    SECTIONS_CONFIG[section]?.component || SECTIONS_CONFIG.general.component;
  return <ActiveComponent />;
};

function App() {
  const { t, i18n } = useTranslation();
  const [onboardingStep, setOnboardingStep] = useState<OnboardingStep | null>(
    null,
  );
  // Track if this is a returning user who just needs to grant permissions
  // (vs a new user who needs full onboarding including model selection)
  const [isReturningUser, setIsReturningUser] = useState(false);
  const [currentSection, setCurrentSection] =
    useState<SidebarSection>("general");
  const { settings, updateSetting } = useSettings();
  const direction = getLanguageDirection(i18n.language);
  const refreshAudioDevices = useSettingsStore(
    (state) => state.refreshAudioDevices,
  );
  const refreshOutputDevices = useSettingsStore(
    (state) => state.refreshOutputDevices,
  );
  const hasCompletedPostOnboardingInit = useRef(false);

  useEffect(() => {
    checkOnboardingStatus();
  }, []);

  // Initialize RTL direction when language changes
  useEffect(() => {
    initializeRTL(i18n.language);
  }, [i18n.language]);

  // Initialize Enigo, shortcuts, and refresh audio devices when main app loads
  useEffect(() => {
    if (onboardingStep === "done" && !hasCompletedPostOnboardingInit.current) {
      hasCompletedPostOnboardingInit.current = true;
      Promise.all([
        commands.initializeEnigo(),
        commands.initializeShortcuts(),
      ]).catch((e) => {
        console.warn("Failed to initialize:", e);
      });
      refreshAudioDevices();
      refreshOutputDevices();
    }
  }, [onboardingStep, refreshAudioDevices, refreshOutputDevices]);

  // Handle keyboard shortcuts for debug mode toggle
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Check for Ctrl+Shift+D (Windows/Linux) or Cmd+Shift+D (macOS)
      const isDebugShortcut =
        event.shiftKey &&
        event.key.toLowerCase() === "d" &&
        (event.ctrlKey || event.metaKey);

      if (isDebugShortcut) {
        event.preventDefault();
        const currentDebugMode = settings?.debug_mode ?? false;
        updateSetting("debug_mode", !currentDebugMode);
      }
    };

    // Add event listener when component mounts
    document.addEventListener("keydown", handleKeyDown);

    // Cleanup event listener when component unmounts
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [settings?.debug_mode, updateSetting]);

  // Listen for recording errors from the backend and show a toast
  useEffect(() => {
    const unlisten = listen<RecordingErrorEvent>("recording-error", (event) => {
      const { error_type, detail } = event.payload;

      if (error_type === "microphone_permission_denied") {
        const currentPlatform = platform();
        const platformKey = `errors.micPermissionDenied.${currentPlatform}`;
        const description = t(platformKey, {
          defaultValue: t("errors.micPermissionDenied.generic"),
        });
        toast.error(t("errors.micPermissionDeniedTitle"), { description });
      } else if (error_type === "no_input_device") {
        toast.error(t("errors.noInputDeviceTitle"), {
          description: t("errors.noInputDevice"),
        });
      } else {
        // detail carries the raw OS-level recording error and is logged to
        // handy.log on the Rust side already — never render it in the toast.
        toast.error(t("errors.recordingFailedGeneric"));
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for paste failures and show a toast.
  // The technical error detail is logged to handy.log on the Rust side
  // (see actions.rs `error!("Failed to paste transcription: ...")`),
  // so we show a localized, user-friendly message here instead of the raw error.
  useEffect(() => {
    const unlisten = listen("paste-error", () => {
      toast.error(t("errors.pasteFailedTitle"), {
        description: t("errors.pasteFailed"),
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for transcription failures and show a toast. The payload carries
  // no error text (see actions.rs) — the raw engine message is logged to
  // handy.log on the Rust side, never sent to the frontend.
  useEffect(() => {
    const unlisten = listen("transcription-error", () => {
      toast.error(t("errors.transcriptionFailedTitle"), {
        description: t("errors.transcriptionFailed"),
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for model loading failures and show a toast. `error` is a raw
  // backend message (logged to handy.log) and is intentionally not rendered.
  useEffect(() => {
    const unlisten = listen<ModelStateEvent>("model-state-changed", (event) => {
      if (event.payload.event_type === "loading_failed") {
        toast.error(
          t("errors.modelLoadFailed", {
            model:
              event.payload.model_name || t("errors.modelLoadFailedUnknown"),
          }),
          {
            description: t("errors.modelLoadFailedHint"),
          },
        );
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  // Listen for categorized diagnostic events and show a toast for the
  // ones that need proactive user attention. Payload is a category enum
  // only (see src-tauri/src/diagnostics.rs) — never raw model/daemon text.
  // Debounced per-category so a flaky connection during one long dictation
  // doesn't spam repeated identical toasts.
  const lastDiagnosticToastRef = useRef<{ category: string; at: number }>({
    category: "",
    at: 0,
  });
  useEffect(() => {
    const DEBOUNCE_MS = 10_000;
    const categoryKey = (category: FailureCategory): string =>
      typeof category === "string"
        ? category
        : `post_process_http_error:${category.post_process_http_error.status_category}`;

    const unlisten = listen<DiagnosticEvent>("diagnostic-event", (event) => {
      const { category } = event.payload;
      const key = categoryKey(category);
      const now = Date.now();
      if (
        lastDiagnosticToastRef.current.category === key &&
        now - lastDiagnosticToastRef.current.at < DEBOUNCE_MS
      ) {
        return;
      }

      switch (key) {
        case "ollama_unreachable":
          toast.error(t("errors.diagnostics.ollamaUnreachableTitle"), {
            description: t("errors.diagnostics.ollamaUnreachableHint"),
          });
          break;
        case "model_missing":
          toast.error(t("errors.diagnostics.modelMissing"));
          break;
        case "whisper_model_not_loaded":
          toast.error(t("errors.diagnostics.whisperModelNotLoaded"));
          break;
        case "accessibility_missing":
          toast.error(t("errors.diagnostics.accessibilityMissingTitle"), {
            description: t("errors.diagnostics.accessibilityMissingHint"),
          });
          break;
        case "post_process_http_error:unreachable":
        case "post_process_http_error:client_error":
        case "post_process_http_error:server_error":
        case "post_process_http_error:timeout":
          toast.error(t("errors.diagnostics.postProcessFailed"));
          break;
        default:
          // mic_permission_denied / no_input_device / transcription_failed
          // already toast via their own listeners above;
          // paste_blocked_secure_field is expected behavior (journal-only).
          break;
      }

      lastDiagnosticToastRef.current = { category: key, at: now };
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  const revealMainWindowForPermissions = async () => {
    try {
      await commands.showMainWindowCommand();
    } catch (e) {
      console.warn("Failed to show main window for permission onboarding:", e);
    }
  };

  const checkOnboardingStatus = async () => {
    try {
      const settingsResult = await commands.getAppSettings();
      const hasCompletedOnboarding =
        settingsResult.status === "ok" &&
        settingsResult.data.onboarding_completed === true;
      const currentPlatform = platform();

      if (hasCompletedOnboarding) {
        // Returning user - check if they need to grant permissions first
        setIsReturningUser(true);

        if (currentPlatform === "macos") {
          try {
            const [hasAccessibility, hasMicrophone] = await Promise.all([
              checkAccessibilityPermission(),
              checkMicrophonePermission(),
            ]);
            if (!hasAccessibility || !hasMicrophone) {
              await revealMainWindowForPermissions();
              setOnboardingStep("accessibility");
              return;
            }
          } catch (e) {
            console.warn("Failed to check macOS permissions:", e);
            // If we can't check, proceed to main app and let them fix it there
          }
        }

        if (currentPlatform === "windows") {
          try {
            const microphoneStatus =
              await commands.getWindowsMicrophonePermissionStatus();
            if (
              microphoneStatus.supported &&
              microphoneStatus.overall_access === "denied"
            ) {
              await revealMainWindowForPermissions();
              setOnboardingStep("accessibility");
              return;
            }
          } catch (e) {
            console.warn("Failed to check Windows microphone permissions:", e);
            // If we can't check, proceed to main app and let them fix it there
          }
        }

        setOnboardingStep("done");
      } else {
        // New user - start full onboarding
        setIsReturningUser(false);
        setOnboardingStep("accessibility");
      }
    } catch (error) {
      console.error("Failed to check onboarding status:", error);
      setOnboardingStep("accessibility");
    }
  };

  const handleAccessibilityComplete = () => {
    // Returning users already have models, skip to main app
    // New users need to select a model
    setOnboardingStep(isReturningUser ? "done" : "model");
  };

  const handleModelSelected = () => {
    // Model download started; offer local cleanup setup next.
    setOnboardingStep("ollama");
  };

  const handleOllamaSetupComplete = () => {
    // Success, skip, or a soft failure all land here — offer the personal
    // dictionary before the main app either way.
    setOnboardingStep("dictionary");
  };

  const handleDictionaryComplete = () => {
    // Personal dictionary is the last setup step; end on the usage walkthrough
    // so new users learn (and try) the core loop before landing in the app.
    setOnboardingStep("usage");
  };

  const handleUsageComplete = () => {
    setOnboardingStep("done");
  };

  // Still checking onboarding status
  if (onboardingStep === null) {
    return null;
  }

  if (onboardingStep === "accessibility") {
    return <AccessibilityOnboarding onComplete={handleAccessibilityComplete} />;
  }

  if (onboardingStep === "model") {
    return <Onboarding onModelSelected={handleModelSelected} />;
  }

  if (onboardingStep === "ollama") {
    return <OllamaOnboarding onComplete={handleOllamaSetupComplete} />;
  }

  if (onboardingStep === "dictionary") {
    return <DictionaryOnboarding onComplete={handleDictionaryComplete} />;
  }

  if (onboardingStep === "usage") {
    return <UsageOnboarding onComplete={handleUsageComplete} />;
  }

  return (
    <div
      dir={direction}
      className="h-screen flex flex-col select-none cursor-default"
    >
      <Toaster
        theme="system"
        toastOptions={{
          unstyled: true,
          classNames: {
            toast:
              "bg-background border border-mid-gray/20 rounded-lg shadow-lg px-4 py-3 flex items-center gap-3 text-sm",
            title: "font-medium",
            description: "text-mid-gray",
          },
        }}
      />
      <WhatsNewGate />
      {/* Main content area that takes remaining space */}
      <div className="flex-1 flex overflow-hidden">
        <Sidebar
          activeSection={currentSection}
          onSectionChange={setCurrentSection}
        />
        {/* Scrollable content area */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="flex-1 overflow-y-auto">
            <div className="flex flex-col items-center p-4 gap-4">
              <AccessibilityPermissions />
              {renderSettingsContent(currentSection)}
            </div>
          </div>
        </div>
      </div>
      {/* Fixed footer at bottom */}
      <Footer />
    </div>
  );
}

export default App;
