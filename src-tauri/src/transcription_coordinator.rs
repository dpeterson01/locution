use crate::actions::ACTION_MAP;
use crate::managers::audio::AudioRecordingManager;
use log::{debug, error, warn};
use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const DEBOUNCE: Duration = Duration::from_millis(30);

struct PendingPress {
    binding_id: String,
    hotkey_string: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BusyAction {
    Ignore,
    Remember,
    Forget,
}

fn classify_busy_input(is_pressed: bool, push_to_talk: bool, remembered: bool) -> BusyAction {
    match (push_to_talk, is_pressed) {
        (false, true) if remembered => BusyAction::Forget,
        (false, true) => BusyAction::Remember,
        (false, false) => BusyAction::Ignore,
        (true, true) => BusyAction::Remember,
        (true, false) if remembered => BusyAction::Forget,
        (true, false) => BusyAction::Ignore,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Stage {
    Idle,
    Recording(String), // binding_id
    Processing,
}

struct InputEvent {
    binding_id: String,
    hotkey_string: String,
    is_pressed: bool,
    push_to_talk: bool,
    external: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Effect {
    Start {
        binding_id: String,
        hotkey_string: String,
    },
    Stop {
        binding_id: String,
        hotkey_string: String,
    },
}

/// Commands processed sequentially by the coordinator thread.
enum Command {
    Input(InputEvent),
    Cancel { recording_was_active: bool },
    ProcessingFinished,
}

struct CoordinatorState {
    stage: Stage,
    last_press: HashMap<String, Instant>,
    pending_press: Option<PendingPress>,
}

impl CoordinatorState {
    fn new() -> Self {
        Self {
            stage: Stage::Idle,
            last_press: HashMap::new(),
            pending_press: None,
        }
    }

    fn on_input(&mut self, input: InputEvent, now: Instant) -> Option<Effect> {
        if input.is_pressed && !input.external {
            if self
                .last_press
                .get(&input.binding_id)
                .is_some_and(|last| now.duration_since(*last) < DEBOUNCE)
            {
                debug!("Debounced press for '{}'", input.binding_id);
                return None;
            }
            self.last_press.insert(input.binding_id.clone(), now);
        }

        if matches!(self.stage, Stage::Processing) {
            if self
                .pending_press
                .as_ref()
                .is_some_and(|pending| pending.binding_id != input.binding_id)
            {
                debug!(
                    "Ignoring input for '{}': another binding is pending",
                    input.binding_id
                );
                return None;
            }

            match classify_busy_input(
                input.is_pressed,
                input.push_to_talk,
                self.pending_press.is_some(),
            ) {
                BusyAction::Remember => {
                    self.pending_press = Some(PendingPress {
                        binding_id: input.binding_id,
                        hotkey_string: input.hotkey_string,
                    });
                }
                BusyAction::Forget => self.pending_press = None,
                BusyAction::Ignore => {}
            }
            return None;
        }

        if input.push_to_talk {
            if input.is_pressed && matches!(self.stage, Stage::Idle) {
                return Some(self.begin_recording(input.binding_id, input.hotkey_string));
            }
            if !input.is_pressed
                && matches!(&self.stage, Stage::Recording(id) if id == &input.binding_id)
            {
                return Some(self.begin_processing(input.binding_id, input.hotkey_string));
            }
        } else if input.is_pressed {
            match &self.stage {
                Stage::Idle => {
                    return Some(self.begin_recording(input.binding_id, input.hotkey_string));
                }
                Stage::Recording(id) if id == &input.binding_id => {
                    return Some(self.begin_processing(input.binding_id, input.hotkey_string));
                }
                _ => debug!("Ignoring press for '{}': pipeline busy", input.binding_id),
            }
        }
        None
    }

    fn on_cancel(&mut self, recording_was_active: bool) {
        self.pending_press = None;
        if !matches!(self.stage, Stage::Processing)
            && (recording_was_active || matches!(self.stage, Stage::Recording(_)))
        {
            self.stage = Stage::Idle;
        }
    }

    fn on_processing_finished(&mut self) -> Option<Effect> {
        self.stage = Stage::Idle;
        let pending = self.pending_press.take()?;
        Some(self.begin_recording(pending.binding_id, pending.hotkey_string))
    }

    fn on_start_result(&mut self, binding_id: &str, started: bool) {
        if !started && matches!(&self.stage, Stage::Recording(id) if id == binding_id) {
            self.stage = Stage::Idle;
        }
    }

    fn begin_recording(&mut self, binding_id: String, hotkey_string: String) -> Effect {
        self.stage = Stage::Recording(binding_id.clone());
        Effect::Start {
            binding_id,
            hotkey_string,
        }
    }

    fn begin_processing(&mut self, binding_id: String, hotkey_string: String) -> Effect {
        self.stage = Stage::Processing;
        Effect::Stop {
            binding_id,
            hotkey_string,
        }
    }
}

/// Serialises all transcription lifecycle events through a single thread
/// to eliminate race conditions between keyboard shortcuts, signals, and
/// the async transcribe-paste pipeline.
pub struct TranscriptionCoordinator {
    tx: Sender<Command>,
}

pub fn is_transcribe_binding(id: &str) -> bool {
    id == "transcribe" || id == "transcribe_with_post_process"
}

impl TranscriptionCoordinator {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut state = CoordinatorState::new();

                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        Command::Input(input) => {
                            if let Some(effect) = state.on_input(input, Instant::now()) {
                                run_effect(&app, &mut state, effect);
                            }
                        }
                        Command::Cancel {
                            recording_was_active,
                        } => state.on_cancel(recording_was_active),
                        Command::ProcessingFinished => {
                            if let Some(effect) = state.on_processing_finished() {
                                run_effect(&app, &mut state, effect);
                            }
                        }
                    }
                }
                debug!("Transcription coordinator exited");
            }));
            if let Err(e) = result {
                error!("Transcription coordinator panicked: {e:?}");
            }
        });

        Self { tx }
    }

    /// Send a keyboard/signal input event for a transcribe binding.
    /// For signal-based toggles, use `is_pressed: true` and `push_to_talk: false`.
    pub fn send_input(
        &self,
        binding_id: &str,
        hotkey_string: &str,
        is_pressed: bool,
        push_to_talk: bool,
    ) {
        self.send(binding_id, hotkey_string, is_pressed, push_to_talk, false);
    }

    pub fn send_external_input(&self, binding_id: &str, source: &str) {
        self.send(binding_id, source, true, false, true);
    }

    fn send(
        &self,
        binding_id: &str,
        hotkey_string: &str,
        is_pressed: bool,
        push_to_talk: bool,
        external: bool,
    ) {
        if self
            .tx
            .send(Command::Input(InputEvent {
                binding_id: binding_id.to_string(),
                hotkey_string: hotkey_string.to_string(),
                is_pressed,
                push_to_talk,
                external,
            }))
            .is_err()
        {
            warn!("Transcription coordinator channel closed");
        }
    }

    pub fn notify_cancel(&self, recording_was_active: bool) {
        if self
            .tx
            .send(Command::Cancel {
                recording_was_active,
            })
            .is_err()
        {
            warn!("Transcription coordinator channel closed");
        }
    }

    pub fn notify_processing_finished(&self) {
        if self.tx.send(Command::ProcessingFinished).is_err() {
            warn!("Transcription coordinator channel closed");
        }
    }
}

fn run_effect(app: &AppHandle, state: &mut CoordinatorState, effect: Effect) {
    match effect {
        Effect::Start {
            binding_id,
            hotkey_string,
        } => {
            let started = start(app, &binding_id, &hotkey_string);
            state.on_start_result(&binding_id, started);
        }
        Effect::Stop {
            binding_id,
            hotkey_string,
        } => stop(app, &binding_id, &hotkey_string),
    }
}

fn start(app: &AppHandle, binding_id: &str, hotkey_string: &str) -> bool {
    let Some(action) = ACTION_MAP.get(binding_id) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return false;
    };
    action.start(app, binding_id, hotkey_string);
    let recording = app
        .try_state::<Arc<AudioRecordingManager>>()
        .is_some_and(|a| a.is_recording());
    if !recording {
        debug!("Start for '{binding_id}' did not begin recording; staying idle");
    }
    recording
}

fn stop(app: &AppHandle, binding_id: &str, hotkey_string: &str) {
    let Some(action) = ACTION_MAP.get(binding_id) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return;
    };
    action.stop(app, binding_id, hotkey_string);
}

#[cfg(test)]
mod tests {
    use super::*;

    const BINDING: &str = "transcribe";
    const OTHER_BINDING: &str = "transcribe_with_post_process";

    fn input(binding_id: &str, is_pressed: bool, push_to_talk: bool, external: bool) -> InputEvent {
        InputEvent {
            binding_id: binding_id.to_string(),
            hotkey_string: binding_id.to_string(),
            is_pressed,
            push_to_talk,
            external,
        }
    }

    fn processing_state() -> CoordinatorState {
        CoordinatorState {
            stage: Stage::Processing,
            last_press: HashMap::new(),
            pending_press: None,
        }
    }

    #[test]
    fn busy_toggle_presses_preserve_parity() {
        let mut state = processing_state();
        let now = Instant::now();

        assert!(state
            .on_input(input(BINDING, true, false, true), now)
            .is_none());
        assert!(state.pending_press.is_some());
        assert!(state
            .on_input(input(BINDING, true, false, true), now + DEBOUNCE)
            .is_none());
        assert!(state.pending_press.is_none());
        assert!(state.on_processing_finished().is_none());
        assert_eq!(state.stage, Stage::Idle);
    }

    #[test]
    fn held_ptt_press_starts_when_processing_finishes() {
        let mut state = processing_state();

        assert!(state
            .on_input(input(BINDING, true, true, false), Instant::now())
            .is_none());
        assert!(matches!(
            state.on_processing_finished(),
            Some(Effect::Start { binding_id, .. }) if binding_id == BINDING
        ));
    }

    #[test]
    fn released_ptt_press_is_forgotten_while_processing() {
        let mut state = processing_state();
        let now = Instant::now();

        state.on_input(input(BINDING, true, true, false), now);
        state.on_input(input(BINDING, false, true, false), now + DEBOUNCE);

        assert!(state.on_processing_finished().is_none());
        assert_eq!(state.stage, Stage::Idle);
    }

    #[test]
    fn different_binding_does_not_replace_pending_press() {
        let mut state = processing_state();
        let now = Instant::now();

        state.on_input(input(BINDING, true, false, true), now);
        state.on_input(input(OTHER_BINDING, true, false, true), now + DEBOUNCE);

        assert!(matches!(
            state.on_processing_finished(),
            Some(Effect::Start { binding_id, .. }) if binding_id == BINDING
        ));
    }

    #[test]
    fn cancel_drops_pending_press() {
        let mut state = processing_state();
        state.on_input(input(BINDING, true, false, true), Instant::now());

        state.on_cancel(false);

        assert!(state.on_processing_finished().is_none());
        assert_eq!(state.stage, Stage::Idle);
    }

    #[test]
    fn external_toggle_edges_are_not_debounced() {
        let mut state = CoordinatorState::new();
        let now = Instant::now();

        assert!(matches!(
            state.on_input(input(BINDING, true, false, true), now),
            Some(Effect::Start { .. })
        ));
        assert!(matches!(
            state.on_input(
                input(BINDING, true, false, true),
                now + Duration::from_millis(5)
            ),
            Some(Effect::Stop { .. })
        ));
    }

    #[test]
    fn keyboard_toggle_edges_keep_per_binding_debounce() {
        let mut state = CoordinatorState::new();
        let now = Instant::now();

        assert!(matches!(
            state.on_input(input(BINDING, true, false, false), now),
            Some(Effect::Start { .. })
        ));
        assert!(state
            .on_input(
                input(BINDING, true, false, false),
                now + Duration::from_millis(5),
            )
            .is_none());
        assert_eq!(state.stage, Stage::Recording(BINDING.to_string()));
    }

    #[test]
    fn failed_start_rolls_back_to_idle() {
        let mut state = CoordinatorState::new();
        state.on_input(input(BINDING, true, false, false), Instant::now());

        state.on_start_result(BINDING, false);

        assert_eq!(state.stage, Stage::Idle);
    }
}
