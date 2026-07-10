import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { CheckCircle2 } from "lucide-react";
import { events, type HistoryUpdatePayload } from "@/bindings";
import OnboardingLockup from "./OnboardingLockup";
import UsageGuide, { HotkeyChips } from "./UsageGuide";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { formatKeyCombination } from "../../lib/utils/keyboard";

interface UsageOnboardingProps {
  onComplete: () => void;
}

/**
 * Final first-run step: teach the core hold → speak → release loop, then let the
 * user actually try it. Success is detected from the transcription pipeline's
 * `history-update-payload` "added" event (the same signal the History view
 * uses), so it fires regardless of which element the paste landed in. The step
 * is never blocking — Continue always works even if practice is skipped or the
 * model is still downloading.
 */
const UsageOnboarding: React.FC<UsageOnboardingProps> = ({ onComplete }) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const osType = useOsType();
  const [practiceText, setPracticeText] = useState("");
  const [succeeded, setSucceeded] = useState(false);

  const rawBinding = settings?.bindings?.transcribe?.current_binding ?? "";
  const hotkey = formatKeyCombination(rawBinding, osType);

  // Keep a ref so the event listener (registered once) always sees the latest
  // success state without re-subscribing on every keystroke.
  const succeededRef = useRef(false);
  succeededRef.current = succeeded;

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    events.historyUpdatePayload
      .listen((event) => {
        const payload: HistoryUpdatePayload = event.payload;
        if (payload.action !== "added") return;

        const text =
          payload.entry.post_processed_text || payload.entry.transcription_text;

        if (!succeededRef.current) {
          setSucceeded(true);
          if (text) {
            // Mirror the transcription into the practice box so there's always
            // a visible result even if the paste target lost focus.
            setPracticeText((prev) => (prev.trim().length > 0 ? prev : text));
          }
        }
      })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return (
    <div className="h-screen w-screen inset-0 flex flex-col gap-4 p-6">
      <div className="flex shrink-0 flex-col items-center gap-2">
        <OnboardingLockup />
        <h2 className="text-lg font-semibold">{t("onboarding.usage.title")}</h2>
        <p className="mx-auto max-w-md text-center font-medium text-text/70">
          {t("onboarding.usage.description")}
        </p>
      </div>

      <div className="mx-auto flex min-h-0 w-full max-w-[640px] flex-1 flex-col gap-4 overflow-y-auto">
        <UsageGuide hotkey={hotkey} />

        <div
          className={`flex flex-col gap-3 rounded-xl border p-4 transition-colors ${
            succeeded
              ? "border-logo-primary/40 bg-logo-primary/5"
              : "border-mid-gray/20 bg-background"
          }`}
        >
          <div className="flex items-center justify-between gap-2">
            <h3 className="text-sm font-semibold text-text">
              {t("onboarding.usage.practice.title")}
            </h3>
            {succeeded && (
              <span className="inline-flex items-center gap-1.5 text-sm font-medium text-logo-primary">
                <CheckCircle2 className="h-4 w-4" />
                {t("onboarding.usage.practice.success")}
              </span>
            )}
          </div>

          {!succeeded && hotkey && (
            <p className="flex flex-wrap items-center gap-1.5 text-xs text-text/70">
              {t("onboarding.usage.practice.promptPrefix")}
              <HotkeyChips hotkey={hotkey} />
              {t("onboarding.usage.practice.promptSuffix")}
            </p>
          )}

          <Textarea
            className="w-full"
            value={practiceText}
            onChange={(e) => setPracticeText(e.target.value)}
            placeholder={t("onboarding.usage.practice.placeholder")}
            rows={3}
            autoFocus
          />

          <p className="text-xs text-text/50">
            {t("onboarding.usage.footerNote")}
          </p>
        </div>
      </div>

      <div className="flex shrink-0 items-center justify-center gap-3 pb-2">
        <Button onClick={onComplete} variant="ghost" size="md">
          {t("onboarding.usage.skip")}
        </Button>
        <Button onClick={onComplete} variant="primary" size="md">
          {succeeded
            ? t("onboarding.usage.finishSuccess")
            : t("onboarding.usage.finish")}
        </Button>
      </div>
    </div>
  );
};

export default UsageOnboarding;
