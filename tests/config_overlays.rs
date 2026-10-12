use kosk::config;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const RELOAD_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

fn wait_for(predicate: impl Fn() -> bool) {
    let deadline = Instant::now() + RELOAD_TIMEOUT;
    while !predicate() {
        assert!(Instant::now() < deadline, "config reload timed out");
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[test]
fn overlay_files_reload_and_removing_declarations_restores_main_config() {
    let directory = std::env::temp_dir().join(format!(
        "kosk-overlay-watcher-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("config.toml");
    let overlay = directory.join("overlay.toml");
    let main =
        "config_version = 3\nscale_x = 36.0\nconfig_overlays = [{ file = 'overlay.toml' }]\n";
    fs::write(&path, main).unwrap();
    fs::write(&overlay, "scale_x = 44.0\nevent_debounce_ms = 120\n").unwrap();
    config::init_from_path(path.clone()).unwrap();
    assert_eq!(config::get().scale_x, 44.0);
    assert_eq!(config::get().event_debounce_ms, 120);

    let reloads = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&reloads);
    config::on_changed(move || {
        observed.fetch_add(1, Ordering::Relaxed);
    })
    .unwrap();
    fs::write(&overlay, "scale_x = 45.0\nevent_debounce_ms = 130\n").unwrap();
    wait_for(|| reloads.load(Ordering::Relaxed) > 0 && config::get().scale_x == 45.0);
    assert_eq!(config::get().event_debounce_ms, 130);
    assert_eq!(fs::read_to_string(&path).unwrap(), main);

    fs::write(&overlay, "scale_x = 'invalid'\n").unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(config::get().scale_x, 45.0);
    fs::write(&overlay, "scale_x = 46.0\n").unwrap();
    wait_for(|| config::get().scale_x == 46.0);

    fs::write(&path, "config_version = 3\nscale_x = 36.0\n").unwrap();
    wait_for(|| config::get().scale_x == 36.0);
    assert_eq!(
        config::get().event_debounce_ms,
        config::Config::default().event_debounce_ms
    );
    fs::remove_dir_all(directory).unwrap();
}
