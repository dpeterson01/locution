import React from "react";
import { useTranslation } from "react-i18next";
import { WordCorrectionThreshold } from "./WordCorrectionThreshold";
import { LogLevelSelector } from "./LogLevelSelector";
import { LiveLogViewer } from "./LiveLogViewer";
import { PasteDelay } from "./PasteDelay";
import { RecordingBuffer } from "./RecordingBuffer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { AlwaysOnMicrophone } from "../AlwaysOnMicrophone";
import { SoundPicker } from "../SoundPicker";
import { ClamshellMicrophoneSelector } from "../ClamshellMicrophoneSelector";
import { UpdateChecksToggle } from "../UpdateChecksToggle";
import { WhatsNewPreview } from "./WhatsNewPreview";
import { ExportDiagnostics } from "./ExportDiagnostics";
import { DebugModeToggle } from "./DebugModeToggle";
import { useSettings } from "../../../hooks/useSettings";

export const DebugSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const debugModeEnabled = getSetting("debug_mode") || false;

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      {/* Standalone toggle, outside any card — enabling it is what makes
          the box below appear, rather than being a row inside that box. */}
      <DebugModeToggle descriptionMode="tooltip" grouped={false} />

      {debugModeEnabled && (
        <SettingsGroup title={t("settings.debug.title")}>
          <div className="settings-expand-in divide-y divide-mid-gray/20">
            <LogLevelSelector grouped={true} />
            <WhatsNewPreview descriptionMode="tooltip" grouped={true} />
            <UpdateChecksToggle descriptionMode="tooltip" grouped={true} />
            <SoundPicker
              label={t("settings.debug.soundTheme.label")}
              description={t("settings.debug.soundTheme.description")}
            />
            <WordCorrectionThreshold descriptionMode="tooltip" grouped={true} />
            <PasteDelay descriptionMode="tooltip" grouped={true} />
            <RecordingBuffer descriptionMode="tooltip" grouped={true} />
            <AlwaysOnMicrophone descriptionMode="tooltip" grouped={true} />
            <ClamshellMicrophoneSelector descriptionMode="tooltip" grouped={true} />
            <LiveLogViewer descriptionMode="tooltip" grouped={true} />
            <ExportDiagnostics descriptionMode="tooltip" grouped={true} />
          </div>
        </SettingsGroup>
      )}
    </div>
  );
};
