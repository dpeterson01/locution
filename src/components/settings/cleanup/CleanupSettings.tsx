import React from "react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { PostProcessingToggle } from "../PostProcessingToggle";
import { OllamaSetupCard } from "../OllamaSetupCard";
import { PostProcessingSettings } from "../post-processing/PostProcessingSettings";
import { useSettings } from "../../../hooks/useSettings";

export const CleanupSettings: React.FC = () => {
  const { getSetting } = useSettings();
  const cleanupEnabled = getSetting("post_process_enabled") || false;

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      {/* Standalone toggle, outside any card — enabling it is what makes
          the configuration below appear. */}
      <PostProcessingToggle descriptionMode="tooltip" grouped={false} />

      {cleanupEnabled && (
        <SettingsGroup>
          <div className="settings-expand-in divide-y divide-mid-gray/20">
            <OllamaSetupCard descriptionMode="tooltip" grouped={true} />

            <div className="settings-expand-in bg-mid-gray/5 p-4">
              <PostProcessingSettings />
            </div>
          </div>
        </SettingsGroup>
      )}
    </div>
  );
};
