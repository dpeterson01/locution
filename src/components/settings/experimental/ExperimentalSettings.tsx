import React from "react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ExperimentalToggle } from "../ExperimentalToggle";
import { PostProcessingToggle } from "../PostProcessingToggle";
import { OllamaSetupCard } from "../OllamaSetupCard";
import { PostProcessingSettings } from "../post-processing/PostProcessingSettings";
import { AccelerationSelector } from "../AccelerationSelector";
import { LazyStreamClose } from "../LazyStreamClose";
import { useSettings } from "../../../hooks/useSettings";

export const ExperimentalSettings: React.FC = () => {
  const { getSetting } = useSettings();
  const experimentalEnabled = getSetting("experimental_enabled") || false;
  const cleanupEnabled = getSetting("post_process_enabled") || false;

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      {/* Standalone toggle, outside any card — enabling it is what makes
          the box below appear, rather than being a row inside that box. */}
      <ExperimentalToggle descriptionMode="tooltip" grouped={false} />

      {experimentalEnabled && (
        <SettingsGroup>
          <div className="settings-expand-in divide-y divide-mid-gray/20">
            <AccelerationSelector descriptionMode="tooltip" grouped={true} />
            <LazyStreamClose descriptionMode="tooltip" grouped={true} />
            <PostProcessingToggle descriptionMode="tooltip" grouped={true} />

            {/* Enabling cleanup expands its full configuration right here,
                beneath its toggle — no separate sidebar tab. */}
            {cleanupEnabled && (
              <div className="settings-expand-in bg-mid-gray/5 p-4">
                <PostProcessingSettings />
              </div>
            )}

            <OllamaSetupCard descriptionMode="tooltip" grouped={true} />
          </div>
        </SettingsGroup>
      )}
    </div>
  );
};
