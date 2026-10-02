use kosk::controller::{ControllerButton, ControllerInput, ControllerKind};
use kosk::{completion, config, state::AppState};

#[derive(Debug, Clone)]
struct Input(Option<ControllerButton>);

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
        self.0 == Some(button)
    }
    fn is_engaged(&self) -> bool {
        self.0.is_some()
    }
    fn family(&self) -> ControllerKind {
        ControllerKind::Sc2
    }
    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

#[test]
fn visibility_clears_completion_and_blocks_input_until_release() {
    let dir = std::env::temp_dir().join(format!("kosk-show-hide-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "config_version = 1\n\
        [key_sink]\ntype = 'log'\nfile = 'keys.log'\n\
        [tips]\ncompletion_next_word_setup_shown = true\n\
        [controller_map.Keyboard]\nfaceLeft = 'sendKey.a'\nfaceTop = 'toggleCtrl'\n",
    )
    .unwrap();
    config::init_from_path(path).unwrap();
    let mut state = AppState::new((1920.0, 1080.0)).unwrap();
    completion::ensure(std::sync::Arc::new(|| {})).unwrap();
    let ctx = egui::Context::default();
    let letter = Input(Some(ControllerButton::FaceLeft));
    let modifier = Input(Some(ControllerButton::FaceTop));
    let idle = Input(None);
    let log_path = dir.join("keys.log");

    state.handle_controller_input(&ctx, &modifier).unwrap();
    state.reset_controller_input(&ctx).unwrap();
    completion::with_mut(|session| {
        let session = session.unwrap();
        session.note_log(completion::LogEvent::Text, "hel");
        session.arm_eat_accept_space();
        session.arm_suggestion_just_accepted();
    });
    state.set_overlay_visible(false);
    assert!(!state.overlay_visible());
    completion::with_mut(|session| {
        let session = session.unwrap();
        assert!(session.typed_text().is_empty());
        assert!(session.candidates().is_empty());
        assert!(!session.suggestion_just_accepted());
        assert!(session.take_eat_accept_space(' ').is_none());
    });
    let before = std::fs::read_to_string(&log_path).unwrap();
    state.handle_controller_input(&ctx, &letter).unwrap();
    state.reset_controller_input(&ctx).unwrap();
    assert_eq!(std::fs::read_to_string(&log_path).unwrap(), before);

    completion::with_mut(|session| session.unwrap().note_log(completion::LogEvent::Text, "old"));
    state.set_overlay_visible(true);
    completion::with_mut(|session| assert!(session.unwrap().typed_text().is_empty()));
    state.handle_controller_input(&ctx, &letter).unwrap();
    state.handle_controller_input(&ctx, &letter).unwrap();
    assert_eq!(std::fs::read_to_string(&log_path).unwrap(), before);

    state.note_controller_idle(&ctx, &idle).unwrap();
    state.handle_controller_input(&ctx, &letter).unwrap();
    let log = std::fs::read_to_string(&log_path).unwrap();
    assert!(log.lines().any(|line| line.ends_with(" text a")), "{log}");
    assert!(!log.contains("Control"), "{log}");
}
