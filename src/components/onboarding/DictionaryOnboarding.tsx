import React from "react";
import { useTranslation } from "react-i18next";
import OnboardingLockup from "./OnboardingLockup";
import { CustomWords } from "../settings/CustomWords";
import { Button } from "../ui/Button";

interface DictionaryOnboardingProps {
  onComplete: () => void;
}

/// Onboarding step exposing the personal dictionary (custom_words): names,
/// jargon, and unusual spellings the transcriber should get right. Reuses the
/// settings CustomWords editor; the step is skippable — Continue always works.
const DictionaryOnboarding: React.FC<DictionaryOnboardingProps> = ({
  onComplete,
}) => {
  const { t } = useTranslation();

  return (
    <div className="h-screen w-screen flex flex-col p-6 gap-4 inset-0">
      <div className="flex flex-col items-center gap-2 shrink-0">
        <OnboardingLockup />
        <h2 className="text-lg font-semibold">
          {t("onboarding.dictionary.title")}
        </h2>
        <p className="text-text/70 max-w-md font-medium mx-auto text-center">
          {t("onboarding.dictionary.description")}
        </p>
      </div>

      <div className="max-w-[600px] w-full mx-auto flex-1 min-h-0 overflow-y-auto">
        <CustomWords descriptionMode="inline" />
      </div>

      <div className="flex justify-center shrink-0 pb-2">
        <Button onClick={onComplete} variant="primary" size="md">
          {t("onboarding.dictionary.continue")}
        </Button>
      </div>
    </div>
  );
};

export default DictionaryOnboarding;
