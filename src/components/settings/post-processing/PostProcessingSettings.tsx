import React, { useEffect, useMemo, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Check, Download, Loader2, RefreshCcw } from "lucide-react";
import { commands, type OllamaAvailability } from "@/bindings";
import { useOllamaModelPull } from "../../../hooks/useOllamaModelPull";

import {
  Dropdown,
  SettingContainer,
  SettingsGroup,
  Textarea,
  ToggleSwitch,
} from "@/components/ui";
import { Button } from "../../ui/Button";
import { ResetButton } from "../../ui/ResetButton";
import { Input } from "../../ui/Input";

import { ProviderSelect } from "../PostProcessingSettingsApi/ProviderSelect";
import { BaseUrlField } from "../PostProcessingSettingsApi/BaseUrlField";
import { ApiKeyField } from "../PostProcessingSettingsApi/ApiKeyField";
import { ModelSelect } from "../PostProcessingSettingsApi/ModelSelect";
import { usePostProcessProviderState } from "../PostProcessingSettingsApi/usePostProcessProviderState";
import { useSettings } from "../../../hooks/useSettings";
import { PostProcessingSettingsVoice } from "./PostProcessingSettingsVoice";
import { PostProcessingSettingsPerApp } from "./PostProcessingSettingsPerApp";

const PostProcessingSettingsApiComponent: React.FC = () => {
  const { t } = useTranslation();
  const state = usePostProcessProviderState();

  return (
    <>
      <SettingContainer
        title={t("settings.postProcessing.api.provider.title")}
        description={t("settings.postProcessing.api.provider.description")}
        descriptionMode="tooltip"
        layout="horizontal"
        grouped={true}
      >
        <div className="flex items-center gap-2">
          <ProviderSelect
            options={state.providerOptions}
            value={state.selectedProviderId}
            onChange={state.handleProviderSelect}
          />
        </div>
      </SettingContainer>

      <>
        {state.selectedProvider?.id === "custom" && (
          <SettingContainer
            title={t("settings.postProcessing.api.baseUrl.title")}
            description={t("settings.postProcessing.api.baseUrl.description")}
            descriptionMode="tooltip"
            layout="horizontal"
            grouped={true}
          >
            <div className="flex items-center gap-2">
              <BaseUrlField
                value={state.baseUrl}
                onBlur={state.handleBaseUrlChange}
                placeholder={t(
                  "settings.postProcessing.api.baseUrl.placeholder",
                )}
                disabled={state.isBaseUrlUpdating}
                className="min-w-[380px]"
              />
            </div>
          </SettingContainer>
        )}

        <SettingContainer
          title={t("settings.postProcessing.api.apiKey.title")}
          description={t("settings.postProcessing.api.apiKey.description")}
          descriptionMode="tooltip"
          layout="horizontal"
          grouped={true}
        >
          <div className="flex items-center gap-2">
            <ApiKeyField
              value={state.apiKey}
              onBlur={state.handleApiKeyChange}
              placeholder={t("settings.postProcessing.api.apiKey.placeholder")}
              disabled={state.isApiKeyUpdating}
              className="min-w-[320px]"
            />
          </div>
        </SettingContainer>
      </>

      {!state.isCustomProvider && (
        <SettingContainer
          title={t("settings.postProcessing.api.model.title")}
          description={
            state.isCustomProvider
              ? t("settings.postProcessing.api.model.descriptionCustom")
              : t("settings.postProcessing.api.model.descriptionDefault")
          }
          descriptionMode="tooltip"
          layout="stacked"
          grouped={true}
        >
          <div className="flex items-center gap-2">
            <ModelSelect
              value={state.model}
              options={state.modelOptions}
              disabled={state.isModelUpdating}
              isLoading={state.isFetchingModels}
              placeholder={
                state.modelOptions.length > 0
                  ? t(
                      "settings.postProcessing.api.model.placeholderWithOptions",
                    )
                  : t("settings.postProcessing.api.model.placeholderNoOptions")
              }
              onSelect={state.handleModelSelect}
              onCreate={state.handleModelCreate}
              onBlur={() => {}}
              className="flex-1 min-w-[380px]"
            />
            <ResetButton
              onClick={state.handleRefreshModels}
              disabled={state.isFetchingModels}
              ariaLabel={t("settings.postProcessing.api.model.refreshModels")}
              className="flex h-10 w-10 items-center justify-center"
            >
              <RefreshCcw
                className={`h-4 w-4 ${state.isFetchingModels ? "animate-spin" : ""}`}
              />
            </ResetButton>
          </div>
        </SettingContainer>
      )}
    </>
  );
};

const PostProcessingSettingsPromptsComponent: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, refreshSettings } = useSettings();
  const [isCreating, setIsCreating] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [draftText, setDraftText] = useState("");
  const [draftUseContext, setDraftUseContext] = useState(false);
  // Which mode the editor is viewing. Purely an editor cursor — it has no
  // bearing on the active/selected mode (that is owned by per-app auto mode and
  // the overlay/tray/cycle surfaces). Seeded to the first mode by the effect
  // below, and reset to null after the edited mode is deleted.
  const [editingPromptId, setEditingPromptId] = useState<string | null>(null);

  const prompts = getSetting("post_process_prompts") || [];
  const selectedPromptId = editingPromptId ?? "";
  const selectedPrompt =
    prompts.find((prompt) => prompt.id === selectedPromptId) || null;

  useEffect(() => {
    if (editingPromptId === null && prompts.length > 0) {
      setEditingPromptId(prompts[0].id);
    }
  }, [editingPromptId, prompts]);

  useEffect(() => {
    if (isCreating) return;

    if (selectedPrompt) {
      setDraftName(selectedPrompt.name);
      setDraftText(selectedPrompt.prompt);
      setDraftUseContext(selectedPrompt.use_context ?? false);
    } else {
      setDraftName("");
      setDraftText("");
      setDraftUseContext(false);
    }
  }, [
    isCreating,
    selectedPromptId,
    selectedPrompt?.name,
    selectedPrompt?.prompt,
    selectedPrompt?.use_context,
  ]);

  const handlePromptSelect = (promptId: string | null) => {
    if (!promptId) return;
    setEditingPromptId(promptId);
    setIsCreating(false);
  };

  const handleCreatePrompt = async () => {
    if (!draftName.trim() || !draftText.trim()) return;

    try {
      const result = await commands.addPostProcessPrompt(
        draftName.trim(),
        draftText.trim(),
        null,
        draftUseContext,
      );
      if (result.status === "ok") {
        await refreshSettings();
        setEditingPromptId(result.data.id);
        setIsCreating(false);
      }
    } catch (error) {
      console.error("Failed to create prompt:", error);
    }
  };

  const handleUpdatePrompt = async () => {
    if (!selectedPromptId || !draftName.trim() || !draftText.trim()) return;

    try {
      await commands.updatePostProcessPrompt(
        selectedPromptId,
        draftName.trim(),
        draftText.trim(),
        null,
        draftUseContext,
      );
      await refreshSettings();
    } catch (error) {
      console.error("Failed to update prompt:", error);
    }
  };

  const handleDeletePrompt = async (promptId: string) => {
    if (!promptId) return;

    try {
      await commands.deletePostProcessPrompt(promptId);
      await refreshSettings();
      setEditingPromptId(null);
      setIsCreating(false);
    } catch (error) {
      console.error("Failed to delete prompt:", error);
    }
  };

  const handleCancelCreate = () => {
    setIsCreating(false);
    if (selectedPrompt) {
      setDraftName(selectedPrompt.name);
      setDraftText(selectedPrompt.prompt);
      setDraftUseContext(selectedPrompt.use_context ?? false);
    } else {
      setDraftName("");
      setDraftText("");
      setDraftUseContext(false);
    }
  };

  const handleStartCreate = () => {
    setIsCreating(true);
    setDraftName("");
    setDraftText("");
    setDraftUseContext(false);
  };

  const hasPrompts = prompts.length > 0;
  const isDirty =
    !!selectedPrompt &&
    (draftName.trim() !== selectedPrompt.name ||
      draftText.trim() !== selectedPrompt.prompt.trim() ||
      draftUseContext !== (selectedPrompt.use_context ?? false));

  const contextGloballyEnabled = getSetting("context_capture_enabled") ?? false;

  // Shared per-mode screen-context toggle (edit + create forms). Separated
  // with a top border/spacing from modeModelField above it — it's a property
  // of the mode itself, not of the model picker it happens to render after.
  const modeUseContextField = (
    <div className="space-y-2 flex flex-col border-t border-mid-gray/20 pt-4 mt-2">
      <ToggleSwitch
        checked={draftUseContext}
        onChange={setDraftUseContext}
        label={t("settings.postProcessing.prompts.useContext.label")}
        description={t(
          "settings.postProcessing.prompts.useContext.description",
        )}
        descriptionMode="tooltip"
        grouped={true}
      />
      {draftUseContext && !contextGloballyEnabled && (
        <p className="text-xs text-mid-gray/70">
          {t("settings.postProcessing.prompts.useContext.disabledHint")}
        </p>
      )}
    </div>
  );

  return (
    <SettingContainer
      title={t("settings.postProcessing.prompts.selectedPrompt.title")}
      description={t(
        "settings.postProcessing.prompts.selectedPrompt.description",
      )}
      descriptionMode="tooltip"
      layout="stacked"
      grouped={true}
    >
      <div className="space-y-3">
        <div className="flex gap-2 min-w-0">
          <Dropdown
            selectedValue={selectedPromptId || null}
            options={prompts.map((p) => ({
              value: p.id,
              label: p.name,
            }))}
            onSelect={(value) => handlePromptSelect(value)}
            placeholder={
              prompts.length === 0
                ? t("settings.postProcessing.prompts.noPrompts")
                : t("settings.postProcessing.prompts.selectPrompt")
            }
            disabled={isCreating}
            className="flex-1 min-w-0"
          />
          <Button
            onClick={handleStartCreate}
            variant="primary"
            size="md"
            disabled={isCreating}
            className="shrink-0"
          >
            {t("settings.postProcessing.prompts.createNew")}
          </Button>
        </div>

        {!isCreating && hasPrompts && selectedPrompt && (
          <div className="space-y-3">
            <div className="space-y-2 flex flex-col">
              <label className="text-sm font-semibold">
                {t("settings.postProcessing.prompts.promptLabel")}
              </label>
              <Input
                type="text"
                value={draftName}
                onChange={(e) => setDraftName(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptLabelPlaceholder",
                )}
                variant="compact"
              />
            </div>

            <div className="space-y-2 flex flex-col">
              <label className="text-sm font-semibold">
                {t("settings.postProcessing.prompts.promptInstructions")}
              </label>
              <Textarea
                value={draftText}
                onChange={(e) => setDraftText(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptInstructionsPlaceholder",
                )}
              />
              <p className="text-xs text-mid-gray/70">
                <Trans
                  i18nKey="settings.postProcessing.prompts.promptTip"
                  components={{ code: <code /> }}
                />
              </p>
            </div>

            {modeUseContextField}

            <div className="flex gap-2 pt-2">
              <Button
                onClick={handleUpdatePrompt}
                variant="primary"
                size="md"
                disabled={!draftName.trim() || !draftText.trim() || !isDirty}
              >
                {t("settings.postProcessing.prompts.updatePrompt")}
              </Button>
              {selectedPromptId &&
                !["mode_short_dictation", "mode_long_dictation"].includes(
                  selectedPromptId,
                ) && (
                  <Button
                    onClick={() => handleDeletePrompt(selectedPromptId)}
                    variant="secondary"
                    size="md"
                    disabled={prompts.length <= 1}
                  >
                    {t("settings.postProcessing.prompts.deletePrompt")}
                  </Button>
                )}
            </div>
          </div>
        )}

        {!isCreating && !selectedPrompt && (
          <div className="p-3 bg-mid-gray/5 rounded-md border border-mid-gray/20">
            <p className="text-sm text-mid-gray">
              {hasPrompts
                ? t("settings.postProcessing.prompts.selectToEdit")
                : t("settings.postProcessing.prompts.createFirst")}
            </p>
          </div>
        )}

        {isCreating && (
          <div className="space-y-3">
            <div className="space-y-2 block flex flex-col">
              <label className="text-sm font-semibold text-text">
                {t("settings.postProcessing.prompts.promptLabel")}
              </label>
              <Input
                type="text"
                value={draftName}
                onChange={(e) => setDraftName(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptLabelPlaceholder",
                )}
                variant="compact"
              />
            </div>

            <div className="space-y-2 flex flex-col">
              <label className="text-sm font-semibold">
                {t("settings.postProcessing.prompts.promptInstructions")}
              </label>
              <Textarea
                value={draftText}
                onChange={(e) => setDraftText(e.target.value)}
                placeholder={t(
                  "settings.postProcessing.prompts.promptInstructionsPlaceholder",
                )}
              />
              <p className="text-xs text-mid-gray/70">
                <Trans
                  i18nKey="settings.postProcessing.prompts.promptTip"
                  components={{ code: <code /> }}
                />
              </p>
            </div>

            {modeUseContextField}

            <div className="flex gap-2 pt-2">
              <Button
                onClick={handleCreatePrompt}
                variant="primary"
                size="md"
                disabled={!draftName.trim() || !draftText.trim()}
              >
                {t("settings.postProcessing.prompts.createPrompt")}
              </Button>
              <Button
                onClick={handleCancelCreate}
                variant="secondary"
                size="md"
              >
                {t("settings.postProcessing.prompts.cancel")}
              </Button>
            </div>
          </div>
        )}
      </div>
    </SettingContainer>
  );
};

// A length-tier model input (Fast/Thorough) with an inline Ollama "Download"
// affordance: probe first (never assume Ollama is running), pull if running,
// or offer to start Ollama if not. One useOllamaModelPull instance per field
// so the Fast and Thorough models download independently.
const TierModelField: React.FC<{
  value: string;
  options: { value: string; label: string }[];
  isLoading: boolean;
  disabled: boolean;
  placeholder: string;
  onChange: (value: string) => void;
  trailing?: React.ReactNode;
}> = ({
  value,
  options,
  isLoading,
  disabled,
  placeholder,
  onChange,
  trailing,
}) => {
  const { t } = useTranslation();
  const [modelsPresent, setModelsPresent] = useState<string[]>([]);
  // null until the first probe resolves — lets us show a neutral "checking"
  // state instead of a misleading download button before we know Ollama's
  // status. An empty models_present means "not installed" ONLY when running;
  // when the daemon is down /api/tags returns nothing, which is "can't verify".
  const [availability, setAvailability] = useState<OllamaAvailability | null>(
    null,
  );
  const modelDownload = useOllamaModelPull();
  const pullingModelRef = useRef<string>("");

  useEffect(() => {
    commands.probeOllamaSetup().then((result) => {
      setModelsPresent(result.models_present);
      setAvailability(result.availability);
    });
  }, []);

  useEffect(() => {
    if (modelDownload.state.status === "present") {
      // A successful pull implies Ollama is running.
      setAvailability("running");
      setModelsPresent((prev) =>
        prev.includes(value.trim()) ? prev : [...prev, value.trim()],
      );
    }
  }, [modelDownload.state.status]);

  const trimmed = value.trim();
  // Ollama reports fully-qualified `name:tag` from /api/tags; a bare name the
  // user typed (or a default) implies the `:latest` tag. Normalize both sides
  // so `llama3.2` matches an installed `llama3.2:latest` (and vice versa).
  const normalizeTag = (m: string) => (m.includes(":") ? m : `${m}:latest`);
  const modelPresent =
    trimmed === "" ||
    modelsPresent.some((m) => normalizeTag(m) === normalizeTag(trimmed));
  const ollamaRunning = availability === "running";
  const probeComplete = availability !== null;
  const downloadIsForCurrent = pullingModelRef.current === trimmed;
  const downloadState = downloadIsForCurrent
    ? modelDownload.state
    : { status: "idle" as const };

  const handleStartDownload = () => {
    pullingModelRef.current = trimmed;
    modelDownload.startDownload(trimmed);
  };
  const handleStartOllamaThenDownload = () => {
    pullingModelRef.current = trimmed;
    modelDownload.startOllamaThenDownload(trimmed);
  };
  const handleRetryDownload = () => {
    pullingModelRef.current = trimmed;
    modelDownload.retry(trimmed);
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <ModelSelect
          value={value}
          options={options}
          disabled={disabled}
          isLoading={isLoading}
          placeholder={placeholder}
          onSelect={onChange}
          onCreate={onChange}
          onBlur={() => {}}
          className="flex-1 min-w-[380px]"
        />
        {trailing}
      </div>

      {downloadState.status === "idle" && trimmed !== "" && (
        <>
          {!probeComplete && (
            <div className="flex items-center gap-2 text-xs text-mid-gray/70">
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
              {t("settings.postProcessing.adaptive.download.checking")}
            </div>
          )}

          {probeComplete && ollamaRunning && modelPresent && (
            <div className="flex items-center gap-1.5 text-xs text-mid-gray/70">
              <Check className="w-3.5 h-3.5 text-green-500" />
              {t("settings.postProcessing.adaptive.download.installed")}
            </div>
          )}

          {probeComplete && ollamaRunning && !modelPresent && (
            <Button
              onClick={handleStartDownload}
              variant="secondary"
              size="sm"
              className="self-start"
            >
              <Download className="w-3.5 h-3.5 mr-1.5" />
              {t("settings.postProcessing.adaptive.download.button")}
            </Button>
          )}

          {probeComplete && !ollamaRunning && (
            <div className="flex flex-col gap-2">
              <p className="text-xs text-mid-gray/70">
                {t("settings.postProcessing.adaptive.download.cannotVerify")}
              </p>
              <Button
                onClick={handleStartOllamaThenDownload}
                variant="secondary"
                size="sm"
                className="self-start"
              >
                {t("settings.postProcessing.adaptive.download.startOllama")}
              </Button>
            </div>
          )}
        </>
      )}

      {downloadState.status === "checking" && (
        <div className="flex items-center gap-2 text-xs text-mid-gray/70">
          <Loader2 className="w-3.5 h-3.5 animate-spin" />
          {t("settings.postProcessing.adaptive.download.checking")}
        </div>
      )}

      {downloadState.status === "not_running" && (
        <div className="flex flex-col gap-2">
          <p className="text-xs text-mid-gray/70">
            {t("settings.postProcessing.adaptive.download.notRunning")}
          </p>
          <Button
            onClick={handleStartOllamaThenDownload}
            variant="secondary"
            size="sm"
            className="self-start"
          >
            {t("settings.postProcessing.adaptive.download.startOllama")}
          </Button>
        </div>
      )}

      {downloadState.status === "starting" && (
        <div className="flex items-center gap-2 text-xs text-mid-gray/70">
          <Loader2 className="w-3.5 h-3.5 animate-spin" />
          {t("settings.postProcessing.adaptive.download.starting")}
        </div>
      )}

      {downloadState.status === "pulling" && (
        <div className="flex flex-col gap-1">
          <div className="w-full h-1.5 bg-mid-gray/20 rounded-full overflow-hidden">
            <div
              className="h-full bg-logo-primary rounded-full transition-all duration-300"
              style={{ width: `${downloadState.percentage ?? 0}%` }}
            />
          </div>
          <p className="text-xs text-mid-gray/70">
            {t("settings.postProcessing.adaptive.download.pulling", {
              percentage: downloadState.percentage ?? 0,
            })}
          </p>
        </div>
      )}

      {(downloadState.status === "interrupted" ||
        downloadState.status === "disk_full" ||
        downloadState.status === "no_network") && (
        <div className="flex flex-col gap-2">
          <p className="text-xs text-mid-gray/70">
            {t(
              `settings.postProcessing.adaptive.download.${
                downloadState.status === "interrupted"
                  ? "interrupted"
                  : downloadState.status === "disk_full"
                    ? "diskFull"
                    : "noNetwork"
              }`,
            )}
          </p>
          <Button
            onClick={handleRetryDownload}
            variant="secondary"
            size="sm"
            className="self-start"
          >
            {t("settings.postProcessing.adaptive.download.retry")}
          </Button>
        </div>
      )}

      {downloadState.status === "unknown_model" && (
        <p className="text-xs text-red-400">
          <Trans
            i18nKey="settings.postProcessing.adaptive.download.unknownModel"
            values={{ name: trimmed }}
            components={{
              link: (
                <a
                  href="https://ollama.com/library"
                  target="_blank"
                  rel="noreferrer"
                  className="underline"
                />
              ),
            }}
          />
        </p>
      )}

      {downloadState.status === "other_error" && (
        <p className="text-xs text-red-400">
          {t("settings.postProcessing.adaptive.download.otherError")}
        </p>
      )}
    </div>
  );
};

const PostProcessingSettingsAdaptiveComponent: React.FC = () => {
  const { t } = useTranslation();
  const state = usePostProcessProviderState();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const shortThresholdChars = getSetting("short_threshold_chars") ?? 150;
  const shortModel = getSetting("short_model") ?? "";
  const longModel = getSetting("long_model") ?? "";
  const skipLlmUnderChars = getSetting("skip_llm_under_chars") ?? 0;

  // The provider hook's options only guarantee the primary model is present;
  // make sure the current tier models always appear in their selects too.
  const tierModelOptions = useMemo(() => {
    const seen = new Set(state.modelOptions.map((option) => option.value));
    const options = [...state.modelOptions];
    for (const value of [shortModel, longModel]) {
      const trimmed = value.trim();
      if (!trimmed || seen.has(trimmed)) continue;
      seen.add(trimmed);
      options.push({ value: trimmed, label: trimmed });
    }
    return options;
  }, [state.modelOptions, shortModel, longModel]);

  if (!state.isCustomProvider) {
    return null;
  }

  const handleNumberChange =
    (key: "short_threshold_chars" | "skip_llm_under_chars") =>
    (event: React.ChangeEvent<HTMLInputElement>) => {
      const value = parseInt(event.target.value, 10);
      if (!isNaN(value) && value >= 0) {
        updateSetting(key, value);
      }
    };

  const modelPlaceholder =
    tierModelOptions.length > 0
      ? t("settings.postProcessing.api.model.placeholderWithOptions")
      : t("settings.postProcessing.api.model.placeholderNoOptions");

  return (
    <SettingsGroup title={t("settings.postProcessing.adaptive.title")}>
      <SettingContainer
        title={t("settings.postProcessing.adaptive.threshold.title")}
        description={t(
          "settings.postProcessing.adaptive.threshold.description",
        )}
        descriptionMode="tooltip"
        layout="horizontal"
        grouped={true}
      >
        <Input
          type="number"
          min="0"
          value={shortThresholdChars}
          onChange={handleNumberChange("short_threshold_chars")}
          disabled={isUpdating("short_threshold_chars")}
          className="w-24"
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.postProcessing.adaptive.shortModel.title")}
        description={t(
          "settings.postProcessing.adaptive.shortModel.description",
        )}
        descriptionMode="tooltip"
        layout="stacked"
        grouped={true}
      >
        <TierModelField
          value={shortModel}
          options={tierModelOptions}
          disabled={isUpdating("short_model")}
          isLoading={state.isFetchingModels}
          placeholder={modelPlaceholder}
          onChange={(value) => updateSetting("short_model", value)}
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.postProcessing.adaptive.longModel.title")}
        description={t(
          "settings.postProcessing.adaptive.longModel.description",
        )}
        descriptionMode="tooltip"
        layout="stacked"
        grouped={true}
      >
        <TierModelField
          value={longModel}
          options={tierModelOptions}
          disabled={isUpdating("long_model")}
          isLoading={state.isFetchingModels}
          placeholder={modelPlaceholder}
          onChange={(value) => updateSetting("long_model", value)}
          trailing={
            <ResetButton
              onClick={state.handleRefreshModels}
              disabled={state.isFetchingModels}
              ariaLabel={t("settings.postProcessing.api.model.refreshModels")}
              className="flex h-10 w-10 items-center justify-center"
            >
              <RefreshCcw
                className={`h-4 w-4 ${state.isFetchingModels ? "animate-spin" : ""}`}
              />
            </ResetButton>
          }
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.postProcessing.adaptive.skip.title")}
        description={t("settings.postProcessing.adaptive.skip.description")}
        descriptionMode="tooltip"
        layout="horizontal"
        grouped={true}
      >
        <Input
          type="number"
          min="0"
          value={skipLlmUnderChars}
          onChange={handleNumberChange("skip_llm_under_chars")}
          disabled={isUpdating("skip_llm_under_chars")}
          className="w-24"
        />
      </SettingContainer>
    </SettingsGroup>
  );
};

export const PostProcessingSettingsApi = React.memo(
  PostProcessingSettingsApiComponent,
);
PostProcessingSettingsApi.displayName = "PostProcessingSettingsApi";

const PostProcessingSettingsContextComponent: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <SettingsGroup title={t("settings.postProcessing.context.title")}>
      <ToggleSwitch
        checked={getSetting("context_capture_enabled") ?? false}
        onChange={(enabled) =>
          updateSetting("context_capture_enabled", enabled)
        }
        isUpdating={isUpdating("context_capture_enabled")}
        label={t("settings.postProcessing.context.toggle.title")}
        description={t("settings.postProcessing.context.toggle.description")}
        descriptionMode="tooltip"
        grouped={true}
      />
    </SettingsGroup>
  );
};

export const PostProcessingSettingsAdaptive = React.memo(
  PostProcessingSettingsAdaptiveComponent,
);
PostProcessingSettingsAdaptive.displayName = "PostProcessingSettingsAdaptive";

export const PostProcessingSettingsContext = React.memo(
  PostProcessingSettingsContextComponent,
);
PostProcessingSettingsContext.displayName = "PostProcessingSettingsContext";

export const PostProcessingSettingsPrompts = React.memo(
  PostProcessingSettingsPromptsComponent,
);
PostProcessingSettingsPrompts.displayName = "PostProcessingSettingsPrompts";

export const PostProcessingSettings: React.FC = () => {
  const { t } = useTranslation();

  // Rendered inline in the Cleanup tab when cleanup is enabled. The Ollama
  // setup card is not repeated here — it lives in the Cleanup group directly
  // above this block. Cleanup no longer has its own hotkey: it is a behavior
  // toggle on the single dictation hotkey (see PostProcessingToggle), so no
  // shortcut picker is rendered here.
  return (
    <div className="space-y-6">
      <SettingsGroup title={t("settings.postProcessing.api.title")}>
        <PostProcessingSettingsApi />
      </SettingsGroup>

      <PostProcessingSettingsAdaptive />

      <PostProcessingSettingsContext />

      <PostProcessingSettingsVoice />

      <SettingsGroup title={t("settings.postProcessing.prompts.title")}>
        <PostProcessingSettingsPrompts />
      </SettingsGroup>

      <PostProcessingSettingsPerApp />
    </div>
  );
};
