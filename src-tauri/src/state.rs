use std::sync::Mutex;

use opencv::core::Mat;

pub type StateMutex = Mutex<AppState>;

#[derive(Default)]
pub enum AppState {
    #[default]
    Init,
    WithKeyImage {
        key: Mat,
    },
}
