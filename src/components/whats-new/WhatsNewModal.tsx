import React from "react";
import { useTranslation } from "react-i18next";
import { Dialog } from "../ui";
import { Button } from "../ui/Button";
import { MarkdownContent } from "./MarkdownContent";
import type { ReleaseNote } from "./releaseNotes";

interface WhatsNewModalProps {
  note: ReleaseNote;
  open: boolean;
  onDismiss: () => void;
  onShowWalkthrough: () => void;
}

export const WhatsNewModal: React.FC<WhatsNewModalProps> = ({
  note,
  open,
  onDismiss,
  onShowWalkthrough,
}) => {
  const { t } = useTranslation();

  return (
    <Dialog
      open={open}
      title={t("whatsNew.title", { version: note.version })}
      closeLabel={t("common.close")}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onDismiss();
      }}
      footer={
        <>
          <Button
            variant="secondary"
            size="md"
            onClick={onShowWalkthrough}
          >
            {t("whatsNew.showWalkthrough")}
          </Button>
          <Button variant="primary" size="md" onClick={onDismiss}>
            {t("common.close")}
          </Button>
        </>
      }
    >
      <MarkdownContent markdown={note.markdown} />
    </Dialog>
  );
};
