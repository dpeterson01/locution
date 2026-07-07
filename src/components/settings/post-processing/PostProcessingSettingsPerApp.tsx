import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { Dropdown, SettingsGroup, ToggleSwitch } from "@/components/ui";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { useSettings } from "../../../hooks/useSettings";

const PostProcessingSettingsPerAppComponent: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const enabled = getSetting("per_app_auto_mode_enabled") ?? false;
  const prompts = getSetting("post_process_prompts") || [];
  const ruleMap = getSetting("per_app_mode_map") || {};

  const [bundleId, setBundleId] = useState("");
  const [modeId, setModeId] = useState<string | null>(prompts[0]?.id ?? null);
  const [isDetecting, setIsDetecting] = useState(false);

  const modeOptions = prompts.map((p) => ({ value: p.id, label: p.name }));

  const modeName = (id: string | undefined) =>
    prompts.find((p) => p.id === id)?.name ?? id ?? "";

  const handleDetect = async () => {
    setIsDetecting(true);
    try {
      const result = await commands.getFrontmostApp();
      if (result.status === "ok" && result.data) {
        setBundleId(result.data.bundle_id);
        toast.success(
          t("settings.postProcessing.perApp.rules.detected", {
            name: result.data.name,
          }),
        );
      } else {
        toast.error(t("settings.postProcessing.perApp.rules.detectFailed"));
      }
    } finally {
      setIsDetecting(false);
    }
  };

  const handleAddRule = () => {
    const trimmed = bundleId.trim();
    if (!trimmed || !modeId) return;
    updateSetting("per_app_mode_map", { ...ruleMap, [trimmed]: modeId });
    setBundleId("");
  };

  const handleRemoveRule = (id: string) => {
    const next = { ...ruleMap };
    delete next[id];
    updateSetting("per_app_mode_map", next);
  };

  return (
    <SettingsGroup title={t("settings.postProcessing.perApp.title")}>
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("per_app_auto_mode_enabled", value)}
        isUpdating={isUpdating("per_app_auto_mode_enabled")}
        label={t("settings.postProcessing.perApp.toggle.title")}
        description={t("settings.postProcessing.perApp.toggle.description")}
        descriptionMode="tooltip"
        grouped={true}
      />

      {enabled && (
        <div className="settings-expand-in p-4 space-y-3">
          <div className="flex items-center gap-2">
            <Input
              type="text"
              value={bundleId}
              onChange={(e) => setBundleId(e.target.value)}
              placeholder={t(
                "settings.postProcessing.perApp.rules.bundleIdPlaceholder",
              )}
              variant="compact"
              className="flex-1 min-w-0"
            />
            <Button
              onClick={handleDetect}
              variant="secondary"
              size="md"
              disabled={isDetecting}
              className="shrink-0"
            >
              {t("settings.postProcessing.perApp.rules.detectButton")}
            </Button>
          </div>

          <div className="flex items-center gap-2">
            <Dropdown
              options={modeOptions}
              selectedValue={modeId}
              onSelect={setModeId}
              placeholder={t(
                "settings.postProcessing.perApp.rules.modePlaceholder",
              )}
              className="flex-1 min-w-0"
            />
            <Button
              onClick={handleAddRule}
              variant="primary"
              size="md"
              disabled={!bundleId.trim() || !modeId}
              className="shrink-0"
            >
              {t("settings.postProcessing.perApp.rules.add")}
            </Button>
          </div>

          {Object.keys(ruleMap).length > 0 && (
            <div className="space-y-1">
              {Object.entries(ruleMap).map(([id, mode]) => (
                <div
                  key={id}
                  className="flex items-center justify-between gap-2 text-sm bg-mid-gray/5 rounded-md px-3 py-2"
                >
                  <span className="truncate">
                    <span className="font-mono text-xs">{id}</span>
                    <span className="text-mid-gray/70">
                      {" "}
                      → {modeName(mode)}
                    </span>
                  </span>
                  <Button
                    onClick={() => handleRemoveRule(id)}
                    variant="secondary"
                    size="sm"
                    className="shrink-0"
                    aria-label={t(
                      "settings.postProcessing.perApp.rules.remove",
                      {
                        bundleId: id,
                      },
                    )}
                  >
                    <svg
                      className="w-3 h-3"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M6 18L18 6M6 6l12 12"
                      />
                    </svg>
                  </Button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </SettingsGroup>
  );
};

export const PostProcessingSettingsPerApp = React.memo(
  PostProcessingSettingsPerAppComponent,
);
PostProcessingSettingsPerApp.displayName = "PostProcessingSettingsPerApp";
