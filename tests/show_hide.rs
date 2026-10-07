use kosk::controller::{ControllerButton, ControllerInput, ControllerKind};
use kosk::{
    completion, config,
    state::{AppState, StateId},
};

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
    for (file, key) in [("main.toml", "a"), ("alternate.toml", "x")] {
        std::fs::write(
            dir.join(file),
            format!("[[rows]]\nindent = 0.0\nitems = [{{ key = '{key}' }}]\n"),
        )
        .unwrap();
    }
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "start_hidden = false\nevent_debounce_ms = 0\nrecord_file = 'record-%.kosk'\n\
        [layouts]\nmain = 'main.toml'\nalternate = 'alternate.toml'\n\
        [key_sink]\ntype = 'log'\nfile = 'keys.log'\n\
        [tips]\ncompletion_next_word_setup_shown = true\n\
        [completion.user_cache]\nenabled = false\n\
        [controller_map.Keyboard]\nfaceLeft = 'sendKey.a'\nfaceTop = 'toggleCtrl'\nquickAccess = 'toggleOverlayVisibility'\n\
        dpadLeft = 'sendKey.left'\nfaceRight = 'switchState.textInput'\nfaceBottom = 'switchLayout.alternate'\n\
        shoulderLeft = 'switchState.settings'\nshoulderRight = 'toggleRecord'\n\
        [controller_map.TextInput]\nfaceTop = 'submit'\n\
        [controller_map.Settings]\nfaceBottom = 'activate'\nshoulderRight = 'toggleOverlayVisibility'\n",
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
    completion::with_mut(|session| session.unwrap().toggle_armed());
    completion::with_mut(|session| assert!(!session.unwrap().armed()));
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
    completion::with_mut(|session| {
        let session = session.unwrap();
        assert!(session.typed_text().is_empty());
        assert_eq!(session.armed(), session.cfg().keyboard.start_armed);
    });
    state.handle_controller_input(&ctx, &letter).unwrap();
    state.handle_controller_input(&ctx, &letter).unwrap();
    assert_eq!(std::fs::read_to_string(&log_path).unwrap(), before);

    state.note_controller_idle(&ctx, &idle).unwrap();
    state.handle_controller_input(&ctx, &letter).unwrap();
    let log = std::fs::read_to_string(&log_path).unwrap();
    assert!(log.lines().any(|line| line.ends_with(" text a")), "{log}");
    assert!(!log[before.len()..].contains("Control"), "{log}");

    let visibility = Input(Some(ControllerButton::QuickAccess));
    let changes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = changes.clone();
    state.on_overlay_visibility_changed(std::sync::Arc::new(move |visible, _| {
        observed.lock().unwrap().push(visible)
    }));
    state.handle_controller_input(&ctx, &visibility).unwrap();
    assert!(!state.overlay_visible());
    state.handle_controller_input(&ctx, &visibility).unwrap();
    assert!(!state.overlay_visible());
    state.note_controller_idle(&ctx, &idle).unwrap();
    state.handle_controller_input(&ctx, &visibility).unwrap();
    assert!(state.overlay_visible());
    assert_eq!(*changes.lock().unwrap(), [false, true]);

    state.note_controller_idle(&ctx, &idle).unwrap();
    state
        .handle_controller_input(&ctx, &Input(Some(ControllerButton::FaceBottom)))
        .unwrap();
    state.note_controller_idle(&ctx, &idle).unwrap();
    state
        .handle_controller_input(&ctx, &Input(Some(ControllerButton::FaceRight)))
        .unwrap();
    assert_eq!(state.current_state(), StateId::TextInput);
    state.note_controller_idle(&ctx, &idle).unwrap();
    state.handle_controller_input(&ctx, &letter).unwrap();
    state.set_overlay_visible(false);
    state.set_overlay_visible(true);
    assert_eq!(state.current_state(), StateId::Keyboard);

    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| state.draw_ui(&ctx, ui));
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text() == "a"
    )));
    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text() == "x"
    )));
    output.textures_delta.clear();

    state.note_controller_idle(&ctx, &idle).unwrap();
    state
        .handle_controller_input(&ctx, &Input(Some(ControllerButton::FaceRight)))
        .unwrap();
    state.note_controller_idle(&ctx, &idle).unwrap();
    state.handle_controller_input(&ctx, &letter).unwrap();
    state.note_controller_idle(&ctx, &idle).unwrap();
    let before_submit = std::fs::read_to_string(&log_path).unwrap();
    state.handle_controller_input(&ctx, &modifier).unwrap();
    let after_submit = std::fs::read_to_string(&log_path).unwrap();
    let submitted = &after_submit[before_submit.len()..];
    assert!(
        submitted.lines().any(|line| line.ends_with(" text a")),
        "{submitted}"
    );
    assert!(!submitted.contains("text aa"), "{submitted}");

    state.note_controller_idle(&ctx, &idle).unwrap();
    state
        .handle_controller_input(&ctx, &Input(Some(ControllerButton::ShoulderLeft)))
        .unwrap();
    assert_eq!(state.current_state(), StateId::Settings);
    state.note_controller_idle(&ctx, &idle).unwrap();
    for _ in 0..4 {
        state
            .handle_controller_input(&ctx, &Input(Some(ControllerButton::DpadDown)))
            .unwrap();
        state.reset_controller_input(&ctx).unwrap();
    }
    state
        .handle_controller_input(&ctx, &Input(Some(ControllerButton::FaceBottom)))
        .unwrap();
    state.reset_controller_input(&ctx).unwrap();
    let settings_visibility = Input(Some(ControllerButton::ShoulderRight));
    state
        .handle_controller_input(&ctx, &settings_visibility)
        .unwrap();
    assert!(!state.overlay_visible());
    state
        .handle_controller_input(&ctx, &settings_visibility)
        .unwrap();
    assert!(!state.overlay_visible());
    state.note_controller_idle(&ctx, &idle).unwrap();
    state
        .handle_controller_input(&ctx, &settings_visibility)
        .unwrap();
    assert!(state.overlay_visible());
    assert_eq!(state.current_state(), StateId::Keyboard);

    state.note_controller_idle(&ctx, &idle).unwrap();
    state
        .handle_controller_input(&ctx, &Input(Some(ControllerButton::ShoulderLeft)))
        .unwrap();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| state.draw_ui(&ctx, ui));
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text() == "Move window"
    )));
    output.textures_delta.clear();

    state.set_overlay_visible(false);
    state.set_overlay_visible(true);
    state.note_controller_idle(&ctx, &idle).unwrap();
    state
        .handle_controller_input(&ctx, &Input(Some(ControllerButton::ShoulderRight)))
        .unwrap();
    assert!(kosk::controller::record::session().is_recording());
    state.set_overlay_visible(false);
    assert!(!kosk::controller::record::session().is_recording());
    state.set_overlay_visible(true);

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

        struct RestoreCursor(POINT);
        impl Drop for RestoreCursor {
            fn drop(&mut self) {
                unsafe {
                    SetCursorPos(self.0.x, self.0.y);
                }
            }
        }
        let mut original = POINT::default();
        assert_ne!(unsafe { GetCursorPos(&mut original) }, 0);
        let _restore = RestoreCursor(original);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(20.0, 20.0));
        unsafe {
            SetCursorPos(100, 100);
        }
        state.set_overlay_visible(false);
        state.set_overlay_visible(true);
        let first = state.get_position(rect, 1.0);
        state.set_overlay_visible(false);
        unsafe {
            SetCursorPos(500, 100);
        }
        state.set_overlay_visible(true);
        let second = state.get_position(rect, 1.0);
        assert_ne!(first, second, "show must refresh the mouse position");
    }
}
