import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";

interface SnippetsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const Snippets: React.FC<SnippetsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const [newTrigger, setNewTrigger] = useState("");
    const [newExpansion, setNewExpansion] = useState("");
    const snippets = getSetting("snippets") || [];

    const handleAddSnippet = () => {
      const trigger = newTrigger.trim();
      const expansion = newExpansion.trim();
      if (!trigger || !expansion) return;

      if (
        snippets.some((s) => s.trigger.toLowerCase() === trigger.toLowerCase())
      ) {
        toast.error(t("settings.advanced.snippets.duplicate", { trigger }));
        return;
      }

      updateSetting("snippets", [...snippets, { trigger, expansion }]);
      setNewTrigger("");
      setNewExpansion("");
    };

    const handleRemoveSnippet = (trigger: string) => {
      updateSetting(
        "snippets",
        snippets.filter((s) => s.trigger !== trigger),
      );
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAddSnippet();
      }
    };

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.snippets.title")}
          description={t("settings.advanced.snippets.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex items-center gap-2">
            <Input
              type="text"
              className="max-w-32"
              value={newTrigger}
              onChange={(e) => setNewTrigger(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder={t("settings.advanced.snippets.triggerPlaceholder")}
              variant="compact"
              disabled={isUpdating("snippets")}
            />
            <Input
              type="text"
              className="max-w-48"
              value={newExpansion}
              onChange={(e) => setNewExpansion(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder={t("settings.advanced.snippets.expansionPlaceholder")}
              variant="compact"
              disabled={isUpdating("snippets")}
            />
            <Button
              onClick={handleAddSnippet}
              disabled={
                !newTrigger.trim() ||
                !newExpansion.trim() ||
                isUpdating("snippets")
              }
              variant="primary"
              size="md"
            >
              {t("settings.advanced.snippets.add")}
            </Button>
          </div>
        </SettingContainer>
        {snippets.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-col gap-1`}
          >
            {snippets.map((snippet) => (
              <div
                key={snippet.trigger}
                className="flex items-center justify-between gap-2 text-sm"
              >
                <span className="truncate">
                  <span className="font-semibold">{snippet.trigger}</span>
                  <span className="text-mid-gray/70">
                    {" "}
                    → {snippet.expansion}
                  </span>
                </span>
                <Button
                  onClick={() => handleRemoveSnippet(snippet.trigger)}
                  disabled={isUpdating("snippets")}
                  variant="secondary"
                  size="sm"
                  className="shrink-0"
                  aria-label={t("settings.advanced.snippets.remove", {
                    trigger: snippet.trigger,
                  })}
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
      </>
    );
  },
);
