import React from "react";
import { useTranslation } from "react-i18next";
import { Dialog } from "../ui";
import UsageGuide from "./UsageGuide";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { formatKeyCombination } from "../../lib/utils/keyboard";

interface UsageGuideModalProps {
  open: boolean;
  onClose: () => void;
}

/**
 * Replay of the first-run "how it works" walkthrough, surfaced from Settings so
 * the guide stays available after onboarding and after major updates. Reuses the
 * same `UsageGuide` content the onboarding step renders (minus the live practice
 * box, which only makes sense on first run).
 */
export const UsageGuideModal: React.FC<UsageGuideModalProps> = ({
  open,
  onClose,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const osType = useOsType();

  const rawBinding = settings?.bindings?.transcribe?.current_binding ?? "";
  const hotkey = formatKeyCombination(rawBinding, osType);

  return (
    <Dialog
      open={open}
      title={t("onboarding.usage.title")}
      description={t("onboarding.usage.description")}
      closeLabel={t("common.close")}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onClose();
      }}
    >
      <div className="py-1">
        <UsageGuide hotkey={hotkey} />
        <p className="mt-4 text-xs text-text/60">
          {t("onboarding.usage.footerNote")}
        </p>
      </div>
    </Dialog>
  );
};

export default UsageGuideModal;
