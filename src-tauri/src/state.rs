use std::sync::Mutex;

use opencv::core::Mat;

pub type StateMutex = Mutex<AppState>;

#[macro_export]
macro_rules! signal {
    ($app: ident, $message_key: expr, $message: expr) => {
        _ = $app.emit($message_key.into(), $message)
    };
}

#[derive(Default)]
pub enum AppState {
    #[default]
    Init,
    WithKeyImage {
        key: Mat,
    },
    WithKeyAndSheets {
        key: Mat,
    },
}

pub enum SignalKeys {
    KeyStatus,
    KeyImage,
    SheetStatus,
    SheetImages,
}
impl From<SignalKeys> for &str {
    fn from(value: SignalKeys) -> Self {
        match value {
            SignalKeys::KeyStatus => "key-status",
            SignalKeys::KeyImage => "key-image",
            SignalKeys::SheetStatus => "sheet-status",
            SignalKeys::SheetImages => "sheet-images",
        }
    }
}
