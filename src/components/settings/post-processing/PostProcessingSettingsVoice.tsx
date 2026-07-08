import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { SettingsGroup, Textarea, ToggleSwitch } from "@/components/ui";
import { Button } from "../../ui/Button";
import { useSettings } from "../../../hooks/useSettings";

const PostProcessingSettingsVoiceComponent: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const styleCardEnabled = getSetting("style_card_enabled") ?? true;
  const styleCard = getSetting("style_card") ?? "";

  const [draftCard, setDraftCard] = useState(styleCard);
  const [samples, setSamples] = useState("");
  const [isDistilling, setIsDistilling] = useState(false);

  useEffect(() => {
    setDraftCard(styleCard);
  }, [styleCard]);

  const handleSaveCard = () => {
    updateSetting("style_card", draftCard.trim());
  };

  const handleDistill = async () => {
    const sampleList = samples
      .split(/\n\s*\n/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (sampleList.length === 0) {
      toast.error(t("settings.postProcessing.voice.card.distill.noSamples"));
      return;
    }

    setIsDistilling(true);
    try {
      const result = await commands.distillStyleCard(sampleList);
      if (result.status === "ok") {
        setDraftCard(result.data);
        updateSetting("style_card", result.data);
        toast.success(t("settings.postProcessing.voice.card.distill.success"));
      } else {
        toast.error(result.error);
      }
    } finally {
      setIsDistilling(false);
    }
  };

  const cardDirty = draftCard.trim() !== styleCard.trim();

  return (
    <SettingsGroup title={t("settings.postProcessing.voice.title")}>
      <ToggleSwitch
        checked={styleCardEnabled}
        onChange={(enabled) => updateSetting("style_card_enabled", enabled)}
        isUpdating={isUpdating("style_card_enabled")}
        label={t("settings.postProcessing.voice.card.toggle.title")}
        description={t("settings.postProcessing.voice.card.toggle.description")}
        descriptionMode="tooltip"
        grouped={true}
      />

      {styleCardEnabled && (
        <div className="settings-expand-in space-y-2 flex flex-col p-4">
        <label className="text-sm font-semibold">
          {t("settings.postProcessing.voice.card.textLabel")}
        </label>
        <Textarea
          value={draftCard}
          onChange={(e) => setDraftCard(e.target.value)}
          placeholder={t("settings.postProcessing.voice.card.textPlaceholder")}
          disabled={!styleCardEnabled}
        />
        <div className="flex gap-2 pt-1">
          <Button
            onClick={handleSaveCard}
            variant="primary"
            size="md"
            disabled={!cardDirty || isUpdating("style_card")}
          >
            {t("settings.postProcessing.voice.card.save")}
          </Button>
        </div>

        <div className="border-t border-mid-gray/20 pt-4 mt-2 space-y-2">
          <label className="text-sm font-semibold">
            {t("settings.postProcessing.voice.card.distill.label")}
          </label>
          <p className="text-xs text-mid-gray/70">
            {t("settings.postProcessing.voice.card.distill.description")}
          </p>
          <Textarea
            value={samples}
            onChange={(e) => setSamples(e.target.value)}
            placeholder={t(
              "settings.postProcessing.voice.card.distill.placeholder",
            )}
            disabled={isDistilling}
            className="w-full"
          />
          <Button
            onClick={handleDistill}
            variant="secondary"
            size="md"
            disabled={isDistilling || !samples.trim()}
          >
            {isDistilling
              ? t("settings.postProcessing.voice.card.distill.distilling")
              : t("settings.postProcessing.voice.card.distill.button")}
          </Button>
        </div>
      </div>
      )}
    </SettingsGroup>
  );
};

export const PostProcessingSettingsVoice = React.memo(
  PostProcessingSettingsVoiceComponent,
);
PostProcessingSettingsVoice.displayName = "PostProcessingSettingsVoice";
