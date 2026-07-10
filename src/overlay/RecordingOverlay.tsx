import { listen } from "@tauri-apps/api/event";
import { Check, Settings as SettingsIcon, X } from "lucide-react";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import "./RecordingOverlay.css";
import { commands, events } from "@/bindings";
import type {
  StreamPhase,
  StreamPhaseEvent,
  StreamTextEvent,
  StreamWorkKind,
} from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import { getLanguageDirection } from "@/lib/utils/rtl";

type OverlayState =
  | "idle"
  | "recording"
  | "streaming"
  | "transcribing"
  | "processing"
  // Short-lived terminal states flashed on the compact overlay after the
  // processing spinner: cleanup ran, cleanup was skipped (plain transcript),
  // or cleanup failed and the raw transcript was kept.
  | "cleaned"
  | "transcribed"
  | "failed";

// Number of reactive bars in the waveform (the simple, smoothed style shared by
// every overlay form). Mic levels arrive as 16 FFT buckets; we take the first N.
const WAVE_BARS = 9;

// Window footprints for the interaction states. A transparent window swallows
// clicks over its whole rect, so the idle window hugs the visible pill and only
// grows while hovered / while the mode popover is open. Sizing goes through the
// resize_overlay command so Rust re-centers the bottom/top anchor.
// IDLE is the minimum/reset footprint and must match OVERLAY_IDLE_WIDTH/HEIGHT
// in src-tauri/src/overlay.rs; with per-app auto-mode on, the resting pill
// carries the active mode's name and the width grows to hug it (measured).
const SIZE_IDLE: [number, number] = [96, 26];
// Window-vs-pill slack for the labeled idle pill (shadow room) — same ratio as
// the default 96 window around the 72px dash pill. The width cap is the CSS
// pill maximum plus that slack: 20 padding + 12 dash + 6 gap + 2 border plus
// the 120px .sidle-mode-name ellipsis cap = 160, + 24 slack = 184. Keep in
// sync with the CSS safety net.
const IDLE_SLACK = 24;
const IDLE_LABEL_MAX_W = 184;
const SIZE_CONTROLS: [number, number] = [224, 54];
const SIZE_COMPACT: [number, number] = [256, 46];
// Recording/transcribing pill with the hover controls card stacked next to it.
const COMPACT_HOVER_H = 100;
const MENU_WIDTH = 240;
const MENU_ROW_H = 30;
const MENU_CHROME_H = 54 + 18; // controls row + popover padding/gap

type ModeEntry = { id: string; name: string; model: string | null };

// The two protected adaptive-tier modes run length-based cleanup (the Short vs
// Long tier is chosen when you dictate, not now), so the resting pill can't
// honestly name one — it shows "Auto" instead. Mirror of PROTECTED_MODE_IDS in
// src-tauri/src/settings.rs.
const ADAPTIVE_MODE_IDS = ["mode_short_dictation", "mode_long_dictation"];

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  const [levels, setLevels] = useState<number[]>(Array(WAVE_BARS).fill(0));
  const [streamText, setStreamText] = useState<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [phase, setPhase] = useState<StreamPhase>("listening");
  const [workKind, setWorkKind] = useState<StreamWorkKind>("transcribing");
  const [elapsed, setElapsed] = useState(0);
  // Bumped on each new streaming session so the Live card remounts fresh (replays
  // the pop-in, and never animates in from the previous panel's open size).
  const [session, setSession] = useState(0);
  // Overlay placement (top vs bottom of the screen). The Live panel grows downward
  // from a top overlay (oldest line under the pill) and upward from a bottom one.
  const [position, setPosition] = useState<"top" | "bottom">("bottom");
  // True once live text overflows the cap. A top overlay fades its top edge only
  // while overflowing, so the resting first line stays crisp flush under the pill.
  const [overflowing, setOverflowing] = useState(false);

  const smoothedLevelsRef = useRef<number[]>(Array(16).fill(0));
  // Live-text scroll-back: the text region "sticks" to the newest line while the
  // user is at the bottom; if they scroll up to read history, auto-follow pauses
  // until they scroll back down.
  const capRef = useRef<HTMLDivElement>(null);
  const pinnedRef = useRef(true);
  const direction = getLanguageDirection(i18n.language);

  // Hover controls (Modes / Record / Settings) + mode popover state.
  // The overlay panel is non-activating and never becomes key, so AppKit
  // doesn't deliver the mouseMoved stream WebKit needs for CSS :hover. Hover
  // is instead driven by polling the cursor position from Rust
  // (overlay_cursor_position) and applying .hovered classes ourselves.
  const [hovering, setHovering] = useState(false);
  const [hoverKey, setHoverKey] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [modes, setModes] = useState<ModeEntry[]>([]);
  const [selectedModeId, setSelectedModeId] = useState<string | null>(null);
  const [overlayStyle, setOverlayStyle] = useState<string>("minimal");
  // Measured width of the labeled idle pill, so the native window can hug it
  // (see the SIZE_IDLE comment: transparent windows swallow clicks).
  const idlePillRef = useRef<HTMLDivElement>(null);
  const [idlePillW, setIdlePillW] = useState(0);
  // Bumped on every show-overlay: Rust resets the window to the state's fixed
  // dimensions on show, so the resize effect must re-run (and re-send) even
  // when none of its other inputs changed.
  const [showTick, setShowTick] = useState(0);
  // Set when Record is clicked so the full recording bar shows even though
  // the cursor is still on the pill; cleared once the cursor leaves.
  const suppressHoverRef = useRef(false);
  const lastSizeRef = useRef<[number, number] | null>(null);

  const syncModeState = async () => {
    try {
      const settings = await commands.getAppSettings();
      if (settings.status === "ok") {
        const s = settings.data;
        setModes(
          (s.post_process_prompts ?? []).map((p) => ({
            id: p.id,
            name: p.name,
            model: p.model ?? null,
          })),
        );
        setSelectedModeId(s.post_process_selected_prompt_id ?? null);
        // Placement drives edge anchoring and which side the popover opens
        // on; sync it here too so a position change in Settings applies
        // immediately instead of on the next show-overlay event.
        setPosition(s.overlay_position === "top" ? "top" : "bottom");
        setOverlayStyle((s.overlay_style as string) ?? "minimal");
      }
    } catch {
      // Keep previous mode state if settings can't be read.
    }
  };

  useEffect(() => {
    const setupEventListeners = async () => {
      const unlistenShow = await listen("show-overlay", async (event) => {
        await syncLanguageFromSettings();
        // The Live panel flows downward from a top overlay and upward from a
        // bottom one; read the placement so the layout can flip to match.
        try {
          const settings = await commands.getAppSettings();
          if (settings.status === "ok") {
            setPosition(
              settings.data.overlay_position === "top" ? "top" : "bottom",
            );
          }
        } catch {
          // Keep the previous/default placement if settings can't be read.
        }
        const overlayState = event.payload as OverlayState;
        setState(overlayState);
        setMenuOpen(false);
        setHovering(false);
        // Rust resets the window to overlay_dimensions(state) on every show,
        // so the size dedupe must forget what it last sent — otherwise a
        // labeled idle pill stays clipped at the native 96×26 reset.
        lastSizeRef.current = null;
        setShowTick((tick) => tick + 1);
        void syncModeState();
        if (overlayState === "recording" || overlayState === "streaming") {
          setStreamText({ committed: "", tentative: "" });
        }
        if (overlayState === "streaming") {
          setPhase("listening");
          setWorkKind("transcribing");
          setElapsed(0);
          setSession((s) => s + 1); // remount the card fresh for this session
        }
        setIsVisible(true);
      });

      const unlistenHide = await listen("hide-overlay", () => {
        setIsVisible(false);
        setMenuOpen(false);
        setHovering(false);
      });

      const unlistenSettings = await listen("settings-changed", () => {
        void syncModeState();
      });

      const unlistenLevel = await listen<number[]>("mic-level", (event) => {
        const newLevels = event.payload as number[];
        // Exponential smoothing across the 16 buckets, then take the first N
        // bars for the shared waveform.
        const smoothed = smoothedLevelsRef.current.map((prev, i) => {
          const target = newLevels[i] || 0;
          return prev * 0.7 + target * 0.3;
        });
        smoothedLevelsRef.current = smoothed;
        setLevels(smoothed.slice(0, WAVE_BARS));
      });

      const unlistenStream = await events.streamTextEvent.listen((event) => {
        setStreamText(event.payload);
      });

      const unlistenPhase = await events.streamPhaseEvent.listen((event) => {
        const payload: StreamPhaseEvent = event.payload;
        setPhase(payload.phase);
        if (payload.kind) setWorkKind(payload.kind);
      });

      // Listeners are mounted — let Rust surface the resting idle pill now.
      // (A fixed startup delay on the Rust side races this setup and can
      // emit show-overlay before anyone is listening.)
      void commands.overlayReady();

      return () => {
        unlistenShow();
        unlistenHide();
        unlistenSettings();
        unlistenLevel();
        unlistenStream();
        unlistenPhase();
      };
    };

    setupEventListeners();
    void syncModeState();
  }, []);

  // The active mode, shared by the hover-row badge, the mode popover
  // checkmark, and (under auto-mode) the at-rest label. Derived before the
  // sizing effects below, which need it.
  const selectedMode = modes.find((m) => m.id === selectedModeId);
  // The two protected adaptive-tier modes run length-based cleanup, so the pill
  // shows "Auto" for them rather than a name that is only half the time right.
  const isAdaptiveMode =
    !!selectedMode && ADAPTIVE_MODE_IDS.includes(selectedMode.id);
  const modeDisplayName = selectedMode
    ? isAdaptiveMode
      ? t("overlay.mode.auto")
      : selectedMode.name
    : "";
  // The at-rest pill carries the mode name only under the Live overlay style.
  const idleLabeled = overlayStyle === "live" && !!selectedMode;

  // Measure the labeled idle pill so the resize effect can size the native
  // window to hug it — width follows the mode name instead of a fixed wider
  // footprint that would leave dead click-swallowing margins.
  useLayoutEffect(() => {
    const el = idlePillRef.current;
    if (!el) return;
    const w = Math.ceil(el.offsetWidth);
    if (w > 0) setIdlePillW((prev) => (prev === w ? prev : w));
  }, [state, hovering, menuOpen, idleLabeled, modeDisplayName]);

  // Window footprint follows the interaction state. The mode popover needs the
  // tallest window; hover needs the controls row; idle shrinks back to the
  // dash (or the measured mode-name pill under auto-mode). Recording states
  // are sized by Rust (show_overlay_state), so we only send sizes while idle
  // or while the popover forces extra height.
  useEffect(() => {
    let desired: [number, number] | null = null;
    if (state === "idle") {
      const restIdle: [number, number] =
        idleLabeled && idlePillW > 0
          ? [
              Math.min(
                Math.max(idlePillW + IDLE_SLACK, SIZE_IDLE[0]),
                IDLE_LABEL_MAX_W,
              ),
              SIZE_IDLE[1],
            ]
          : SIZE_IDLE;
      desired = menuOpen
        ? [MENU_WIDTH, MENU_CHROME_H + modes.length * MENU_ROW_H]
        : hovering
          ? SIZE_CONTROLS
          : restIdle;
    } else if (state !== "streaming") {
      desired = menuOpen
        ? [
            Math.max(MENU_WIDTH, SIZE_COMPACT[0]),
            COMPACT_HOVER_H + 12 + modes.length * MENU_ROW_H,
          ]
        : hovering
          ? [SIZE_COMPACT[0], COMPACT_HOVER_H]
          : SIZE_COMPACT;
    }
    if (!desired) return;
    const last = lastSizeRef.current;
    if (last && last[0] === desired[0] && last[1] === desired[1]) return;
    lastSizeRef.current = desired;
    void commands.resizeOverlay(desired[0], desired[1]);
  }, [
    state,
    hovering,
    menuOpen,
    modes.length,
    idleLabeled,
    idlePillW,
    showTick,
  ]);

  // Cursor poll (~12 Hz while the overlay is visible): drives hovering,
  // per-element hover highlight, and popover dismissal when the cursor
  // leaves. Deliberately not event-based — see the comment on hovering.
  useEffect(() => {
    if (!isVisible || state === "streaming") {
      setHovering(false);
      setHoverKey(null);
      return;
    }
    const id = window.setInterval(async () => {
      try {
        const point = await commands.overlayCursorPosition();
        if (!point) {
          suppressHoverRef.current = false;
          setHovering(false);
          setHoverKey(null);
          setMenuOpen(false);
          return;
        }
        if (suppressHoverRef.current) {
          setHovering(false);
          setHoverKey(null);
          return;
        }
        setHovering(true);
        const el = document.elementFromPoint(point[0], point[1]);
        setHoverKey(el?.closest("[data-hk]")?.getAttribute("data-hk") ?? null);
      } catch {
        // Ignore transient IPC failures; next tick retries.
      }
    }, 80);
    return () => window.clearInterval(id);
  }, [isVisible, state]);

  const handleModeSelect = (id: string) => {
    setSelectedModeId(id); // optimistic; settings-changed confirms
    setMenuOpen(false);
    void commands.setPostProcessSelectedPrompt(id);
  };

  const handleRecordToggle = () => {
    setMenuOpen(false);
    // Show the full recording bar right away instead of keeping the controls
    // under the (still-hovering) cursor; hover comes back once the pointer
    // leaves and returns.
    if (state === "idle") {
      suppressHoverRef.current = true;
      setHovering(false);
      setHoverKey(null);
    }
    void commands.toggleRecording(true);
  };

  const handleExpandToggle = () => {
    const next = overlayStyle === "live" ? "minimal" : "live";
    setOverlayStyle(next); // optimistic; settings-changed confirms
    setMenuOpen(false);
    void commands.changeOverlayStyleSetting(next);
  };

  const handleOpenSettings = () => {
    setMenuOpen(false);
    setHovering(false);
    void commands.showMainWindowCommand();
  };

  // Elapsed timer while the Live overlay is visible.
  useEffect(() => {
    if (state !== "streaming" || !isVisible) return;
    const id = setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => clearInterval(id);
  }, [state, isVisible]);

  // Stick to the bottom as text streams in — but only while pinned, so a user who
  // has scrolled up to read history isn't yanked back down by the next chunk.
  useLayoutEffect(() => {
    const el = capRef.current;
    if (!el) return;
    // Fade the top edge only once text actually overflows the cap.
    setOverflowing(el.scrollHeight > el.clientHeight + 1);
    if (pinnedRef.current) el.scrollTop = el.scrollHeight;
  }, [streamText]);

  // Each fresh streaming session starts pinned to the bottom, fade cleared.
  useEffect(() => {
    pinnedRef.current = true;
    setOverflowing(false);
  }, [session]);

  // Re-pin when the user is within ~a line of the bottom; unpin otherwise.
  const handleStreamScroll = () => {
    const el = capRef.current;
    if (!el) return;
    pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 16;
  };

  const fmtTime = (s: number) =>
    `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

  // ---- Shared building blocks (one visual language for every overlay form) ----
  const waveform = (
    <div className="swave">
      {levels.map((v, i) => (
        <i
          key={i}
          style={{
            height: `${Math.max(3, Math.min(18, 3 + Math.pow(v, 0.7) * 15))}px`,
          }}
        />
      ))}
    </div>
  );

  const cancelBtn = (
    <button
      data-hk="cancel"
      className={`sx ${hoverKey === "cancel" ? "hovered" : ""}`}
      aria-label="cancel"
      onClick={() => commands.cancelOperation()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4 4 L12 12 M12 4 L4 12"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </svg>
    </button>
  );

  const isRecordingState = state !== "idle";

  // Read-only active-mode label: "never silent" means a per-app rule's mode
  // pick must be visible without opening the Modes popover, so it lives in
  // the always-hover-revealed controls row instead. Unlike the old Style
  // badge, this always shows — a mode is always selected (there's no "off"
  // state), so there's no inert/grayed variant to compute.
  // Compact badge (first letter), not the full name — the row's fixed native
  // window footprint (SIZE_CONTROLS) has no room for a text label, and every
  // other affordance here is icon+tooltip already, so this matches the
  // established idiom rather than introducing a new one.
  const modeLabel = selectedMode && (
    <span
      className="sctrl-mode-label"
      title={t("overlay.mode.activeHint", { name: modeDisplayName })}
      aria-label={t("overlay.mode.activeHint", { name: modeDisplayName })}
    >
      {modeDisplayName.charAt(0).toUpperCase()}
    </span>
  );

  // Modes (✦) | Record (Locution bars / stop) | Settings (⤢) — the hover
  // controls. Native title tooltips (WKWebView renders them outside the
  // window bounds, which a popover inside this tiny window could not).
  const controlsRow = (
    <div className="sctrl" role="toolbar">
      {isRecordingState && <span className="sdot sctrl-live" />}
      <button
        data-hk="modes"
        className={`sctrl-btn ${menuOpen ? "active" : ""} ${hoverKey === "modes" ? "hovered" : ""}`}
        title={t("overlay.controls.changeMode")}
        aria-label={t("overlay.controls.changeMode")}
        onClick={() => setMenuOpen((open) => !open)}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path
            d="M8 1.5 L9.6 6.4 L14.5 8 L9.6 9.6 L8 14.5 L6.4 9.6 L1.5 8 L6.4 6.4 Z"
            fill="currentColor"
          />
        </svg>
      </button>
      {modeLabel}
      <button
        data-hk="record"
        className={`sctrl-btn ${hoverKey === "record" ? "hovered" : ""}`}
        title={
          isRecordingState
            ? t("overlay.controls.stop")
            : t("overlay.controls.record")
        }
        aria-label={
          isRecordingState
            ? t("overlay.controls.stop")
            : t("overlay.controls.record")
        }
        onClick={handleRecordToggle}
      >
        {isRecordingState ? (
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <rect
              x="4"
              y="4"
              width="8"
              height="8"
              rx="1.5"
              fill="currentColor"
            />
          </svg>
        ) : (
          // The Locution mark: the app's 5-bar waveform (LocutionMark.tsx),
          // scaled into the 16px icon grid.
          <svg viewBox="0 0 126 135" aria-hidden="true">
            <rect
              x="4"
              y="47"
              width="18"
              height="41"
              rx="9"
              fill="currentColor"
            />
            <rect
              x="30"
              y="30"
              width="18"
              height="75"
              rx="9"
              fill="currentColor"
            />
            <rect
              x="56"
              y="4"
              width="18"
              height="127"
              rx="9"
              fill="currentColor"
            />
            <rect
              x="82"
              y="30"
              width="18"
              height="75"
              rx="9"
              fill="currentColor"
            />
            <rect
              x="108"
              y="47"
              width="18"
              height="41"
              rx="9"
              fill="currentColor"
            />
          </svg>
        )}
      </button>
      <button
        data-hk="expand"
        className={`sctrl-btn ${hoverKey === "expand" ? "hovered" : ""}`}
        title={
          overlayStyle === "live"
            ? t("overlay.controls.collapse")
            : t("overlay.controls.expand")
        }
        aria-label={
          overlayStyle === "live"
            ? t("overlay.controls.collapse")
            : t("overlay.controls.expand")
        }
        onClick={handleExpandToggle}
      >
        {overlayStyle === "live" ? (
          // Inward arrows: collapse the Live panel back to the Minimal pill
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path
              d="M6.5 3 L6.5 6.5 L3 6.5 M6.5 6.5 L2.5 2.5 M9.5 13 L9.5 9.5 L13 9.5 M9.5 9.5 L13.5 13.5"
              stroke="currentColor"
              strokeWidth="1.4"
              strokeLinecap="round"
              strokeLinejoin="round"
              fill="none"
            />
          </svg>
        ) : (
          // Outward arrows: enlarge to the Live panel
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path
              d="M9.5 3 L13 3 L13 6.5 M13 3 L9 7 M6.5 13 L3 13 L3 9.5 M3 13 L7 9"
              stroke="currentColor"
              strokeWidth="1.4"
              strokeLinecap="round"
              strokeLinejoin="round"
              fill="none"
            />
          </svg>
        )}
      </button>
      <button
        data-hk="settings"
        className={`sctrl-btn ${hoverKey === "settings" ? "hovered" : ""}`}
        title={t("overlay.controls.settings")}
        aria-label={t("overlay.controls.settings")}
        onClick={handleOpenSettings}
      >
        <SettingsIcon strokeWidth={1.8} aria-hidden="true" />
      </button>
    </div>
  );

  // Dark popover listing the modes, checkmark on the active one. Rendered
  // away from the screen edge — above the pill for a bottom overlay, below it
  // for a top overlay; the window grows to fit (resize effect + JSX order).
  const modeMenu = menuOpen && (
    <div className="smenu" role="menu">
      {modes.map((mode) => (
        <button
          key={mode.id}
          data-hk={`m:${mode.id}`}
          className={`smenu-item ${mode.id === selectedModeId ? "selected" : ""} ${hoverKey === `m:${mode.id}` ? "hovered" : ""}`}
          role="menuitemradio"
          aria-checked={mode.id === selectedModeId}
          onClick={() => handleModeSelect(mode.id)}
        >
          <span className="smenu-check">
            {mode.id === selectedModeId && (
              <svg viewBox="0 0 16 16" aria-hidden="true">
                <path
                  d="M3 8.5 L6.5 12 L13 4.5"
                  stroke="currentColor"
                  strokeWidth="1.8"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  fill="none"
                />
              </svg>
            )}
          </span>
          <span className="smenu-name">{mode.name}</span>
        </button>
      ))}
    </div>
  );

  // dot (left) | waveform (center) | timer + cancel (right) — same structure for
  // pill & panel, so the Live morph is a pure width change.
  const listeningRow = (showTimer: boolean, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">
        <span className="sdot" />
      </div>
      {waveform}
      <div className="sbase-r">
        {showTimer && <span className="stimer">{fmtTime(elapsed)}</span>}
        {showCancel && cancelBtn}
      </div>
    </div>
  );

  // spinner (left) | label (center) | cancel (right) — same 3-zone grid as the
  // listening row, so the label is centered.
  const workingRow = (label: string, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">
        <span className="sspinner" />
      </div>
      <span className="swork-label">{label}</span>
      <div className="sbase-r">{showCancel && cancelBtn}</div>
    </div>
  );

  // Terminal flash: a checkmark (cleaned / transcribed) or an error glyph
  // (cleanup failed) plus a label, in the same 3-zone grid as workingRow so it
  // swaps in place. No cancel button — the work is already done.
  const terminalRow = (label: string, failed: boolean) => (
    <div className="sbase">
      <div className="sbase-l">
        {failed ? (
          <X
            className="sterminal-icon sterminal-failed"
            size={14}
            strokeWidth={2.75}
          />
        ) : (
          <Check
            className="sterminal-icon sterminal-cleaned"
            size={14}
            strokeWidth={2.75}
          />
        )}
      </div>
      <span className="swork-label">{label}</span>
      <div className="sbase-r" />
    </div>
  );

  // ---- Idle overlay: tiny dash pill; hover grows it into the controls row ----
  if (state === "idle") {
    return (
      <div
        dir={direction}
        className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""}`}
      >
        <div className="sidle-stack">
          {position === "bottom" && modeMenu}
          {hovering ? (
            <div className="scard sctrl-card">{controlsRow}</div>
          ) : idleLabeled && selectedMode ? (
            // Live overlay style: the resting pill names the active mode so it's
            // visible without hover. Protected adaptive-tier modes show "Auto"
            // because the Short/Long tier is only decided when you dictate.
            <div
              ref={idlePillRef}
              className="sidle labeled"
              title={t("overlay.mode.activeHint", { name: modeDisplayName })}
              aria-label={t("overlay.mode.activeHint", {
                name: modeDisplayName,
              })}
            >
              <span className="sidle-dash" />
              <span className="sidle-mode-name">{modeDisplayName}</span>
            </div>
          ) : (
            <div className="sidle" aria-label={t("overlay.idleHint")}>
              <span className="sidle-dash" />
            </div>
          )}
          {position === "top" && modeMenu}
        </div>
      </div>
    );
  }

  // ---- Live overlay: a pill that sculpts open into a panel ----
  if (state === "streaming") {
    const hasText =
      streamText.committed.length > 0 || streamText.tentative.length > 0;
    const working = phase === "working";
    // Keep the panel open whenever there's text — even while finalizing — so the
    // transcript stays put under a working spinner instead of collapsing and
    // squishing the text mid-stream. Only fall back to the small working pill
    // when there was no text to preserve.
    const open = hasText;
    const collapsed = working && !hasText;

    return (
      <div dir={direction} className={`ov-stage ${position}`}>
        <div
          key={session}
          className={`scard ${open ? "open" : ""} ${collapsed ? "working" : ""} ${
            isVisible ? "" : "leaving"
          }`}
        >
          <div className="stext">
            <div className="stext-clip">
              <div
                className={`stext-cap ${overflowing ? "overflowing" : ""}`}
                ref={capRef}
                onScroll={handleStreamScroll}
              >
                <p>
                  <span className="committed">
                    {streamText.committed ? streamText.committed + " " : ""}
                  </span>
                  <span className="tentative">{streamText.tentative}</span>
                  {/* Drop the blinking caret once finalizing — it's no longer
                      capturing, and a static spinner conveys the work. */}
                  {!working && <span className="scaret" />}
                </p>
              </div>
            </div>
          </div>
          {working
            ? workingRow(
                workKind === "polishing"
                  ? t("overlay.processing")
                  : t("overlay.transcribing"),
                true,
              )
            : listeningRow(open, true)}
        </div>
      </div>
    );
  }

  // ---- Minimal overlay: exactly one row at a time — waveform (recording), or a
  // spinner + label (transcribing / processing), or a terminal flash
  // (cleaned / transcribed / cleanup failed). Never more than one. The pill
  // animates its width between them; the cancel button is in both work rows so
  // it stays put.
  const working = state === "transcribing" || state === "processing";
  const terminal =
    state === "cleaned" || state === "transcribed" || state === "failed";
  const workLabel =
    state === "processing"
      ? t("overlay.processing")
      : t("overlay.transcribing");
  const terminalLabel =
    state === "cleaned"
      ? t("overlay.cleaned")
      : state === "failed"
        ? t("overlay.cleanupFailed")
        : t("overlay.transcribed");

  return (
    <div
      dir={direction}
      className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""}`}
    >
      <div className="sidle-stack">
        {position === "bottom" && modeMenu}
        {position === "bottom" && hovering && (
          <div className="scard sctrl-card">{controlsRow}</div>
        )}
        <div
          className={`scard compact ${(working || terminal) && isVisible ? "cworking" : ""}`}
        >
          {terminal
            ? terminalRow(terminalLabel, state === "failed")
            : working
              ? workingRow(workLabel, true)
              : listeningRow(false, true)}
        </div>
        {position === "top" && hovering && (
          <div className="scard sctrl-card">{controlsRow}</div>
        )}
        {position === "top" && modeMenu}
      </div>
    </div>
  );
};

export default RecordingOverlay;
