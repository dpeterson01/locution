import React from "react";
import { useTranslation } from "react-i18next";
import { Hand, Mic, ClipboardCheck } from "lucide-react";

/**
 * Render a formatted hotkey (e.g. "option + space") as a row of keycap chips.
 * `hotkey` is already localized/OS-formatted by `formatKeyCombination`, so we
 * just split on the " + " separator that helper emits.
 */
export const HotkeyChips: React.FC<{
  hotkey: string;
  className?: string;
}> = ({ hotkey, className = "" }) => {
  const parts = hotkey ? hotkey.split(" + ") : [];

  if (parts.length === 0) {
    return null;
  }

  return (
    <span className={`inline-flex items-center gap-1 ${className}`}>
      {parts.map((part, index) => (
        <React.Fragment key={`${part}-${index}`}>
          {index > 0 && <span className="text-text/40 text-xs">+</span>}
          <kbd className="inline-flex items-center rounded-md border border-mid-gray/30 bg-mid-gray/10 px-2 py-0.5 text-xs font-semibold text-text shadow-sm">
            {part}
          </kbd>
        </React.Fragment>
      ))}
    </span>
  );
};

interface UsageGuideProps {
  hotkey: string;
}

/**
 * The reusable "how the app works" explainer: the core hold → speak → release
 * loop rendered as three cards. Presentational only — used both by the
 * first-run usage step and by the replay modal in Settings, so it stays free of
 * onboarding-specific state.
 */
export const UsageGuide: React.FC<UsageGuideProps> = ({ hotkey }) => {
  const { t } = useTranslation();

  const steps = [
    {
      key: "hold",
      icon: Hand,
      title: t("onboarding.usage.steps.hold.title"),
      body: t("onboarding.usage.steps.hold.body"),
      accent: true,
    },
    {
      key: "speak",
      icon: Mic,
      title: t("onboarding.usage.steps.speak.title"),
      body: t("onboarding.usage.steps.speak.body"),
      accent: false,
    },
    {
      key: "release",
      icon: ClipboardCheck,
      title: t("onboarding.usage.steps.release.title"),
      body: t("onboarding.usage.steps.release.body"),
      accent: false,
    },
  ] as const;

  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
      {steps.map((step, index) => {
        const Icon = step.icon;
        return (
          <div
            key={step.key}
            className="relative flex flex-col gap-2 rounded-xl border border-mid-gray/20 bg-background p-4 text-left"
          >
            <div className="flex items-center gap-2">
              <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-logo-primary/15 text-logo-primary">
                <Icon className="h-4 w-4" />
              </span>
              <span className="text-xs font-medium text-text/50">
                {index + 1}
              </span>
            </div>
            <h3 className="text-sm font-semibold text-text">{step.title}</h3>
            {step.accent ? (
              <div className="flex flex-col gap-2">
                <HotkeyChips hotkey={hotkey} />
                <p className="text-xs leading-relaxed text-text/70">
                  {step.body}
                </p>
              </div>
            ) : (
              <p className="text-xs leading-relaxed text-text/70">
                {step.body}
              </p>
            )}
          </div>
        );
      })}
    </div>
  );
};

export default UsageGuide;
