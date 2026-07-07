import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { SettingContainer } from "../../ui/SettingContainer";

interface ExportDiagnosticsProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const ExportDiagnostics: React.FC<ExportDiagnosticsProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const [exporting, setExporting] = useState(false);

  const handleExport = async () => {
    const defaultPath = `locution-diagnostics-${Date.now()}.tar.gz`;
    const destPath = await save({
      defaultPath,
      filters: [{ name: "Diagnostics archive", extensions: ["tar.gz"] }],
    });
    if (!destPath) return;

    setExporting(true);
    try {
      const result = await commands.exportDiagnostics(destPath);
      if (result.status === "ok") {
        toast.success(t("settings.debug.exportDiagnostics.success"));
      } else {
        toast.error(t("settings.debug.exportDiagnostics.failure"));
      }
    } catch (err) {
      console.error("Failed to export diagnostics:", err);
      toast.error(t("settings.debug.exportDiagnostics.failure"));
    } finally {
      setExporting(false);
    }
  };

  return (
    <SettingContainer
      title={t("settings.debug.exportDiagnostics.title")}
      description={t("settings.debug.exportDiagnostics.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <button
        onClick={handleExport}
        disabled={exporting}
        className="px-3 py-1.5 text-sm font-medium bg-mid-gray/10 border border-mid-gray/80 rounded cursor-pointer hover:bg-logo-primary/10 hover:border-logo-primary disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {exporting
          ? t("settings.debug.exportDiagnostics.exporting")
          : t("settings.debug.exportDiagnostics.button")}
      </button>
    </SettingContainer>
  );
};
