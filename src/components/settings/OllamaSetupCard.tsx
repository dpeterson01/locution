import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { SettingContainer } from "../ui/SettingContainer";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { useSettings } from "../../hooks/useSettings";
import OllamaOnboarding from "../onboarding/OllamaOnboarding";

interface OllamaSetupCardProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/// Resume entry point for the first-run Ollama setup wizard. Renders nothing
/// once setup is completed. Lives under Advanced → Experimental (next to the
/// "Enable cleanup" toggle, cleanup's existing discovery path) and at the top
/// of the Cleanup settings tab.
export const OllamaSetupCard: React.FC<OllamaSetupCardProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting } = useSettings();
    const [dialogOpen, setDialogOpen] = useState(false);
    const [alreadyRunning, setAlreadyRunning] = useState(false);
    const status = getSetting("ollama_setup_status") ?? "not_attempted";

    // "Check on next visit": a lightweight probe each time this section
    // mounts, not a background poller — mirrors the Apple Intelligence
    // availability check's lazy, render-time pattern.
    useEffect(() => {
      if (status === "completed") return;
      let cancelled = false;
      commands.probeOllamaSetup().then((result) => {
        if (!cancelled) setAlreadyRunning(result.availability === "running");
      });
      return () => {
        cancelled = true;
      };
    }, [status]);

    if (status === "completed") return null;

    return (
      <>
        <SettingContainer
          title={t("settings.postProcessing.ollamaSetup.title")}
          description={t("settings.postProcessing.ollamaSetup.description")}
          descriptionMode={descriptionMode}
          layout="horizontal"
          grouped={grouped}
        >
          <Button
            onClick={() => setDialogOpen(true)}
            variant="primary"
            size="sm"
          >
            {alreadyRunning
              ? t("settings.postProcessing.ollamaSetup.finish")
              : t("settings.postProcessing.ollamaSetup.setUp")}
          </Button>
        </SettingContainer>

        <Dialog
          open={dialogOpen}
          onOpenChange={setDialogOpen}
          title={t("settings.postProcessing.ollamaSetup.dialogTitle")}
          closeLabel={t("common.close")}
        >
          <OllamaOnboarding
            embedded
            showLogo={false}
            onComplete={() => setDialogOpen(false)}
          />
        </Dialog>
      </>
    );
  },
);
OllamaSetupCard.displayName = "OllamaSetupCard";
