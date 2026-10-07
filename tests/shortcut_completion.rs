use kosk::controller::{ControllerButton, ControllerInput, ControllerKind};
use kosk::{completion, config, state::AppState};

const FIELD_DETECTION_SETTLE_TIME: std::time::Duration = std::time::Duration::from_millis(500);

#[derive(Debug, Clone)]
struct Input(ControllerButton);

impl ControllerInput for Input {
    fn left_stick_raw(&self) -> (f32, f32) {
        (0.0, 0.0)
    }

    fn right_stick_raw(&self) -> (f32, f32) {
        (0.0, 0.0)
    }

    fn trigger_left(&self) -> Option<u8> {
        None
    }

    fn trigger_right(&self) -> Option<u8> {
        None
    }

    fn query(&self, button: ControllerButton) -> bool {
        self.0 == button
    }

    fn is_engaged(&self) -> bool {
        true
    }

    fn family(&self) -> ControllerKind {
        ControllerKind::Sc2
    }

    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

fn press(state: &mut AppState, ctx: &egui::Context, button: ControllerButton) {
    state.handle_controller_input(ctx, &Input(button)).unwrap();
    state.reset_controller_input(ctx).unwrap();
}

#[test]
fn shortcuts_preserve_completion_history_and_release_modifiers() {
    let dir = std::env::temp_dir().join(format!("kosk-shortcut-completion-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("layout.toml"),
        "[[rows]]\nindent = 0.0\nitems = [{ key = 'a' }]\n",
    )
    .unwrap();
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "start_hidden = false\nevent_debounce_ms = 0\n\
        [layouts]\nmain = 'layout.toml'\n\
        [key_sink]\ntype = 'log'\nfile = 'keys.log'\n\
        [tips]\ncompletion_next_word_setup_shown = true\n\
        [completion]\nbackend = 'dictionary'\n\
        [completion.user_cache]\nenabled = false\n\
        [controller_map.Keyboard]\n\
        faceTop = 'toggleCtrl'\nshoulderLeft = 'toggleAlt'\n\
        faceLeft = 'sendKey.a'\nfaceBottom = 'sendKey.backspace'\n\
        faceRight = 'sendKey.enter'\nshoulderRight = 'sendKey.A'\n",
    )
    .unwrap();
    config::init_from_path(path).unwrap();
    let mut state = AppState::new((1920.0, 1080.0)).unwrap();
    completion::ensure(std::sync::Arc::new(|| {})).unwrap();
    std::thread::sleep(FIELD_DETECTION_SETTLE_TIME);
    let ctx = egui::Context::default();
    let log_path = dir.join("keys.log");
    let mut failures = Vec::new();

    for (modifier, name) in [
        (ControllerButton::FaceTop, "Control"),
        (ControllerButton::ShoulderLeft, "Alt"),
    ] {
        for key in [
            ControllerButton::FaceLeft,
            ControllerButton::FaceBottom,
            ControllerButton::FaceRight,
        ] {
            completion::with_mut(|session| {
                let session = session.unwrap();
                session.note_log(completion::LogEvent::Enter, "");
                session.note_log(completion::LogEvent::Text, "hello");
                assert_eq!(session.typed_text(), "hello");
            });
            let before = std::fs::read_to_string(&log_path).unwrap().len();

            press(&mut state, &ctx, modifier);
            press(&mut state, &ctx, key);

            let log = std::fs::read_to_string(&log_path).unwrap();
            let shortcut = &log[before..];
            assert!(
                shortcut.contains(&format!("key {name} Press")),
                "{shortcut}"
            );
            assert!(
                shortcut.contains(&format!("key {name} Release")),
                "{shortcut}"
            );
            completion::with_mut(|session| {
                let session = session.unwrap();
                if session.typed_text() != "hello" || !session.armed() {
                    failures.push(format!("{name}+{key}: {:?}", session.typed_text()));
                }
                session.note_log(completion::LogEvent::Enter, "");
                session.note_log(completion::LogEvent::Text, "hello");
            });

            let before = log.len();
            press(&mut state, &ctx, ControllerButton::ShoulderRight);
            let log = std::fs::read_to_string(&log_path).unwrap();
            let letter = &log[before..];
            assert!(
                letter.lines().any(|line| line.ends_with(" text A")),
                "{letter}"
            );
            assert!(!letter.contains(" Press"), "{letter}");
            completion::with_mut(|session| assert_eq!(session.unwrap().typed_text(), "helloA"));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
