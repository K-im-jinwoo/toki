use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use tauri::image::Image;
use tauri::tray::TrayIcon;

use crate::schema::MascotState;

/// Preloaded sprite frames decoded once at startup to avoid per-tick I/O or PNG decoding.
pub struct SpriteBank {
    sleep: Vec<Image<'static>>,
    idle: Vec<Image<'static>>,
    run: Vec<Image<'static>>,
    sprint: Vec<Image<'static>>,
    alert: Vec<Image<'static>>,
    exhausted: Vec<Image<'static>>,
}

impl SpriteBank {
    pub fn load() -> tauri::Result<Self> {
        Ok(Self {
            sleep: vec![
                decode_sprite(include_bytes!("../../assets/sprites/sleep_1.png"))?,
                decode_sprite(include_bytes!("../../assets/sprites/sleep_2.png"))?,
            ],
            idle: vec![
                decode_sprite(include_bytes!("../../assets/sprites/idle_1.png"))?,
                decode_sprite(include_bytes!("../../assets/sprites/idle_2.png"))?,
            ],
            run: vec![
                decode_sprite(include_bytes!("../../assets/sprites/run_1.png"))?,
                decode_sprite(include_bytes!("../../assets/sprites/run_2.png"))?,
                decode_sprite(include_bytes!("../../assets/sprites/run_3.png"))?,
                decode_sprite(include_bytes!("../../assets/sprites/run_4.png"))?,
            ],
            sprint: vec![
                decode_sprite(include_bytes!("../../assets/sprites/sprint_1.png"))?,
                decode_sprite(include_bytes!("../../assets/sprites/sprint_2.png"))?,
                decode_sprite(include_bytes!("../../assets/sprites/sprint_3.png"))?,
                decode_sprite(include_bytes!("../../assets/sprites/sprint_4.png"))?,
            ],
            alert: vec![
                decode_sprite(include_bytes!("../../assets/sprites/alert_1.png"))?,
                decode_sprite(include_bytes!("../../assets/sprites/alert_2.png"))?,
            ],
            exhausted: vec![decode_sprite(include_bytes!(
                "../../assets/sprites/exhausted_1.png"
            ))?],
        })
    }

    pub fn frames(&self, state: MascotState) -> &[Image<'static>] {
        match state {
            MascotState::Sleep => &self.sleep,
            MascotState::Idle => &self.idle,
            MascotState::Run => &self.run,
            MascotState::Sprint => &self.sprint,
            MascotState::Alert => &self.alert,
            MascotState::Exhausted => &self.exhausted,
        }
    }
}

fn decode_sprite(bytes: &[u8]) -> tauri::Result<Image<'static>> {
    let img = Image::from_bytes(bytes)?;
    Ok(img.to_owned())
}

struct AnimatorInner {
    state: MascotState,
    stopped: bool,
}

/// Thread-safe handle to update mascot state or shut down the background animation loop.
#[derive(Clone)]
pub struct AnimatorHandle {
    shared: Arc<(Mutex<AnimatorInner>, Condvar)>,
}

impl AnimatorHandle {
    pub fn start(tray: TrayIcon, bank: SpriteBank, initial: MascotState) -> Self {
        let shared = Arc::new((
            Mutex::new(AnimatorInner {
                state: initial,
                stopped: false,
            }),
            Condvar::new(),
        ));
        let worker_shared = Arc::clone(&shared);

        std::thread::Builder::new()
            .name("toki-tray-animator".into())
            .spawn(move || {
                let mut frame_idx: usize = 0;
                let mut last_state = initial;

                let (lock, cvar) = &*worker_shared;
                let mut guard = match lock.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };

                loop {
                    if guard.stopped {
                        break;
                    }

                    let current_state = guard.state;
                    if current_state != last_state {
                        frame_idx = 0;
                        last_state = current_state;
                    }

                    let frames = bank.frames(current_state);
                    if !frames.is_empty() {
                        let frame = frames[frame_idx % frames.len()].clone();
                        frame_idx = (frame_idx + 1) % frames.len();
                        drop(guard);
                        #[cfg(target_os = "macos")]
                        let _ = tray.set_icon_with_as_template(Some(frame), true);
                        #[cfg(not(target_os = "macos"))]
                        let _ = tray.set_icon(Some(frame));
                        guard = match lock.lock() {
                            Ok(g) => g,
                            Err(_) => break,
                        };
                    }

                    if guard.stopped {
                        break;
                    }

                    let timeout = Duration::from_millis(guard.state.frame_interval_ms());
                    let (next_guard, _) = match cvar.wait_timeout(guard, timeout) {
                        Ok(res) => res,
                        Err(_) => break,
                    };
                    guard = next_guard;
                }
            })
            .expect("failed to spawn tray animator thread");

        Self { shared }
    }

    pub fn set_state(&self, next: MascotState) {
        let (lock, cvar) = &*self.shared;
        if let Ok(mut guard) = lock.lock() {
            if guard.state != next {
                guard.state = next;
                cvar.notify_one();
            }
        }
    }

    pub fn current_state(&self) -> MascotState {
        let (lock, _) = &*self.shared;
        lock.lock().map(|g| g.state).unwrap_or(MascotState::Idle)
    }

    pub fn stop(&self) {
        let (lock, cvar) = &*self.shared;
        if let Ok(mut guard) = lock.lock() {
            guard.stopped = true;
            cvar.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_bank_loads_all_states() {
        let bank = SpriteBank::load().expect("all embedded sprite PNGs should decode");
        assert_eq!(bank.frames(MascotState::Sleep).len(), 2);
        assert_eq!(bank.frames(MascotState::Idle).len(), 2);
        assert_eq!(bank.frames(MascotState::Run).len(), 4);
        assert_eq!(bank.frames(MascotState::Sprint).len(), 4);
        assert_eq!(bank.frames(MascotState::Alert).len(), 2);
        assert_eq!(bank.frames(MascotState::Exhausted).len(), 1);
        assert_eq!(bank.frames(MascotState::Idle)[0].width(), 56);
        assert_eq!(bank.frames(MascotState::Idle)[0].height(), 44);
    }
}
