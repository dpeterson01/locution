import React, { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { Button } from "../../ui/Button";
import { AppDataDirectory } from "../AppDataDirectory";
import { AppLanguageSelector } from "../AppLanguageSelector";
import { ShowWhatsNewOnUpdate } from "../ShowWhatsNewOnUpdate";
import { LogDirectory } from "../debug";
import { useUpdateChecker } from "../../../hooks/useUpdateChecker";

export const AboutSettings: React.FC = () => {
  const { t } = useTranslation();
  const [version, setVersion] = useState("");
  const updater = useUpdateChecker();

  const updateStatusLabel = () => {
    if (updater.isInstalling) return t("settings.about.version.installing");
    if (updater.isChecking) return t("settings.about.version.checking");
    if (updater.updateAvailable)
      return t("settings.about.version.updateAvailable");
    if (updater.showUpToDate) return t("settings.about.version.upToDate");
    return t("settings.about.version.checkForUpdates");
  };

  useEffect(() => {
    const fetchVersion = async () => {
      try {
        const appVersion = await getVersion();
        setVersion(appVersion);
      } catch (error) {
        console.error("Failed to get app version:", error);
        setVersion("0.1.3");
      }
    };

    fetchVersion();
  }, []);

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.about.title")}>
        <AppLanguageSelector descriptionMode="tooltip" grouped={true} />
        <SettingContainer
          title={t("settings.about.version.title")}
          description={t("settings.about.version.description")}
          grouped={true}
        >
          <div className="flex items-center gap-3">
            {/* eslint-disable-next-line i18next/no-literal-string */}
            <span className="text-sm font-mono">v{version}</span>
            {updater.updateChecksEnabled && (
              <Button
                variant="secondary"
                size="md"
                onClick={
                  updater.updateAvailable
                    ? updater.installUpdate
                    : updater.handleManualUpdateCheck
                }
                disabled={updater.isChecking || updater.isInstalling}
              >
                {updateStatusLabel()}
              </Button>
            )}
          </div>
          <span className="mt-1 block text-xs text-mid-gray">
            {t("settings.about.version.author")}
          </span>
        </SettingContainer>

        <ShowWhatsNewOnUpdate descriptionMode="tooltip" grouped={true} />

        <SettingContainer
          title={t("settings.about.sourceCode.title")}
          description={t("settings.about.sourceCode.description")}
          grouped={true}
        >
          <Button
            variant="secondary"
            size="md"
            onClick={() => openUrl("https://github.com/dpeterson01/locution")}
          >
            {t("settings.about.sourceCode.button")}
          </Button>
        </SettingContainer>

        <AppDataDirectory descriptionMode="tooltip" grouped={true} />
        <LogDirectory grouped={true} />
      </SettingsGroup>
    </div>
  );
};
