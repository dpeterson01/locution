import React, { useEffect, useMemo, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Download, Loader2, RefreshCcw } from "lucide-react";
import { commands } from "@/bindings";
import { useOllamaModelPull } from "../../../hooks/useOllamaModelPull";

import { Alert } from "../../ui/Alert";
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
import { ShortcutInput } from "../ShortcutInput";
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

      {state.isAppleProvider ? (
        state.appleIntelligenceUnavailable ? (
          <Alert variant="error" contained>
            {t("settings.postProcessing.api.appleIntelligence.unavailable")}
          </Alert>
        ) : null
      ) : (
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
                placeholder={t(
                  "settings.postProcessing.api.apiKey.placeholder",
                )}
                disabled={state.isApiKeyUpdating}
                className="min-w-[320px]"
              />
            </div>
          </SettingContainer>
        </>
      )}

      {!state.isAppleProvider && !state.isCustomProvider && (
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
  const { getSetting, updateSetting, isUpdating, refreshSettings } =
    useSettings();
  const providerState = usePostProcessProviderState();
  const [isCreating, setIsCreating] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [draftText, setDraftText] = useState("");
  const [draftModel, setDraftModel] = useState("");
  const [draftUseContext, setDraftUseContext] = useState(false);

  // Locally-pulled Ollama models, for the inline "Download" affordance on
  // the model field below. Probed once on mount ("check on next visit," not
  // a poller) and updated optimistically after a successful download —
  // useOllamaModelPull's own internal probe is authoritative for the actual
  // pull decision, so staleness here only affects whether the button shows.
  const [modelsPresent, setModelsPresent] = useState<string[]>([]);
  const modelDownload = useOllamaModelPull();
  // Which model name the current modelDownload session was started for, so
  // stale progress/error state doesn't render against a different typed name.
  const pullingModelRef = useRef<string>("");

  useEffect(() => {
    commands.probeOllamaSetup().then((result) => {
      setModelsPresent(result.models_present);
    });
  }, []);

  // Intentionally keyed on the download's own status only, not draftModel —
  // this fires once when a pull resolves, not on every keystroke.
  useEffect(() => {
    if (modelDownload.state.status === "present") {
      setModelsPresent((prev) =>
        prev.includes(draftModel.trim()) ? prev : [...prev, draftModel.trim()],
      );
    }
  }, [modelDownload.state.status]);

  const prompts = getSetting("post_process_prompts") || [];
  const selectedPromptId = getSetting("post_process_selected_prompt_id") || "";
  const selectedPrompt =
    prompts.find((prompt) => prompt.id === selectedPromptId) || null;

  // Model options for the per-mode model picker: the provider's fetched list
  // plus whatever the draft currently holds, so the current value always shows.
  const modeModelOptions = useMemo(() => {
    const seen = new Set(providerState.modelOptions.map((o) => o.value));
    const options = [...providerState.modelOptions];
    const trimmed = draftModel.trim();
    if (trimmed && !seen.has(trimmed)) {
      options.push({ value: trimmed, label: trimmed });
    }
    return options;
  }, [providerState.modelOptions, draftModel]);

  useEffect(() => {
    if (isCreating) return;

    if (selectedPrompt) {
      setDraftName(selectedPrompt.name);
      setDraftText(selectedPrompt.prompt);
      setDraftModel(selectedPrompt.model ?? "");
      setDraftUseContext(selectedPrompt.use_context ?? false);
    } else {
      setDraftName("");
      setDraftText("");
      setDraftModel("");
      setDraftUseContext(false);
    }
  }, [
    isCreating,
    selectedPromptId,
    selectedPrompt?.name,
    selectedPrompt?.prompt,
    selectedPrompt?.model,
    selectedPrompt?.use_context,
  ]);

  const handlePromptSelect = (promptId: string | null) => {
    if (!promptId) return;
    updateSetting("post_process_selected_prompt_id", promptId);
    setIsCreating(false);
  };

  const handleCreatePrompt = async () => {
    if (!draftName.trim() || !draftText.trim()) return;

    try {
      const result = await commands.addPostProcessPrompt(
        draftName.trim(),
        draftText.trim(),
        draftModel.trim() || null,
        draftUseContext,
      );
      if (result.status === "ok") {
        await refreshSettings();
        updateSetting("post_process_selected_prompt_id", result.data.id);
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
        draftModel.trim() || null,
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
      setDraftModel(selectedPrompt.model ?? "");
      setDraftUseContext(selectedPrompt.use_context ?? false);
    } else {
      setDraftName("");
      setDraftText("");
      setDraftModel("");
      setDraftUseContext(false);
    }
  };

  const handleStartCreate = () => {
    setIsCreating(true);
    setDraftName("");
    setDraftText("");
    setDraftModel("");
    setDraftUseContext(false);
  };

  const hasPrompts = prompts.length > 0;
  const isDirty =
    !!selectedPrompt &&
    (draftName.trim() !== selectedPrompt.name ||
      draftText.trim() !== selectedPrompt.prompt.trim() ||
      draftModel.trim() !== (selectedPrompt.model ?? "") ||
      draftUseContext !== (selectedPrompt.use_context ?? false));

  const trimmedDraftModel = draftModel.trim();
  const draftModelPresent =
    trimmedDraftModel === "" || modelsPresent.includes(trimmedDraftModel);
  // Only render the download hook's state against the model it was actually
  // started for — if the user edits the field to something else mid-pull,
  // show a plain idle "Download" for the new text instead of stale progress.
  const downloadIsForCurrentDraft =
    pullingModelRef.current === trimmedDraftModel;
  const downloadState = downloadIsForCurrentDraft
    ? modelDownload.state
    : { status: "idle" as const };
  const modelUpdateBlocked =
    downloadIsForCurrentDraft &&
    ["checking", "starting", "pulling"].includes(modelDownload.state.status);

  const handleStartDownload = () => {
    pullingModelRef.current = trimmedDraftModel;
    modelDownload.startDownload(trimmedDraftModel);
  };
  const handleStartOllamaThenDownload = () => {
    pullingModelRef.current = trimmedDraftModel;
    modelDownload.startOllamaThenDownload(trimmedDraftModel);
  };
  const handleRetryDownload = () => {
    pullingModelRef.current = trimmedDraftModel;
    modelDownload.retry(trimmedDraftModel);
  };

  // Shared per-mode model picker (edit + create forms).
  const modeModelField = (
    <div className="space-y-2 flex flex-col">
      <label className="text-sm font-semibold">
        {t("settings.postProcessing.prompts.model.label")}
      </label>
      <ModelSelect
        value={draftModel}
        options={modeModelOptions}
        isLoading={providerState.isFetchingModels}
        placeholder={t("settings.postProcessing.prompts.model.placeholder")}
        onSelect={(value) => setDraftModel(value)}
        onCreate={(value) => setDraftModel(value)}
        onBlur={() => {}}
        className="flex-1 min-w-0"
      />
      <p className="text-xs text-mid-gray/70">
        {t("settings.postProcessing.prompts.model.description")}
      </p>

      {!draftModelPresent && downloadState.status === "idle" && (
        <Button
          onClick={handleStartDownload}
          variant="secondary"
          size="sm"
          className="self-start"
        >
          <Download className="w-3.5 h-3.5 mr-1.5" />
          {t("settings.postProcessing.prompts.model.download.button")}
        </Button>
      )}

      {downloadState.status === "checking" && (
        <div className="flex items-center gap-2 text-xs text-mid-gray/70">
          <Loader2 className="w-3.5 h-3.5 animate-spin" />
          {t("settings.postProcessing.prompts.model.download.checking")}
        </div>
      )}

      {downloadState.status === "not_running" && (
        <div className="flex flex-col gap-2">
          <p className="text-xs text-mid-gray/70">
            {t("settings.postProcessing.prompts.model.download.notRunning")}
          </p>
          <Button
            onClick={handleStartOllamaThenDownload}
            variant="secondary"
            size="sm"
            className="self-start"
          >
            {t("settings.postProcessing.prompts.model.download.startOllama")}
          </Button>
        </div>
      )}

      {downloadState.status === "starting" && (
        <div className="flex items-center gap-2 text-xs text-mid-gray/70">
          <Loader2 className="w-3.5 h-3.5 animate-spin" />
          {t("settings.postProcessing.prompts.model.download.starting")}
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
            {t("settings.postProcessing.prompts.model.download.pulling", {
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
              `settings.postProcessing.prompts.model.download.${
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
            {t("settings.postProcessing.prompts.model.download.retry")}
          </Button>
        </div>
      )}

      {downloadState.status === "unknown_model" && (
        <p className="text-xs text-red-400">
          <Trans
            i18nKey="settings.postProcessing.prompts.model.download.unknownModel"
            values={{ name: trimmedDraftModel }}
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
          {t("settings.postProcessing.prompts.model.download.otherError")}
        </p>
      )}
    </div>
  );

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
            disabled={
              isUpdating("post_process_selected_prompt_id") || isCreating
            }
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

            {modeModelField}

            {modeUseContextField}

            <div className="flex gap-2 pt-2">
              <Button
                onClick={handleUpdatePrompt}
                variant="primary"
                size="md"
                disabled={
                  !draftName.trim() ||
                  !draftText.trim() ||
                  !isDirty ||
                  modelUpdateBlocked
                }
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

            {modeModelField}

            {modeUseContextField}

            <div className="flex gap-2 pt-2">
              <Button
                onClick={handleCreatePrompt}
                variant="primary"
                size="md"
                disabled={
                  !draftName.trim() || !draftText.trim() || modelUpdateBlocked
                }
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
        <div className="flex items-center gap-2">
          <ModelSelect
            value={shortModel}
            options={tierModelOptions}
            disabled={isUpdating("short_model")}
            isLoading={state.isFetchingModels}
            placeholder={modelPlaceholder}
            onSelect={(value) => updateSetting("short_model", value)}
            onCreate={(value) => updateSetting("short_model", value)}
            onBlur={() => {}}
            className="flex-1 min-w-[380px]"
          />
        </div>
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
        <div className="flex items-center gap-2">
          <ModelSelect
            value={longModel}
            options={tierModelOptions}
            disabled={isUpdating("long_model")}
            isLoading={state.isFetchingModels}
            placeholder={modelPlaceholder}
            onSelect={(value) => updateSetting("long_model", value)}
            onCreate={(value) => updateSetting("long_model", value)}
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

  // Rendered inline under Advanced → Experimental when cleanup is enabled
  // (no sidebar tab of its own). The Ollama setup card is not repeated here —
  // it lives in the Experimental group directly above this block.
  return (
    <div className="space-y-6">
      <SettingsGroup title={t("settings.postProcessing.hotkey.title")}>
        <ShortcutInput
          shortcutId="transcribe_with_post_process"
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>

      <SettingsGroup title={t("settings.postProcessing.api.title")}>
        <PostProcessingSettingsApi />
      </SettingsGroup>

      <PostProcessingSettingsAdaptive />

      <PostProcessingSettingsContext />

      <PostProcessingSettingsVoice />

      <PostProcessingSettingsPerApp />

      <SettingsGroup title={t("settings.postProcessing.prompts.title")}>
        <PostProcessingSettingsPrompts />
      </SettingsGroup>
    </div>
  );
};
