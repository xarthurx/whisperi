pub mod app;
pub mod audio;
pub mod clipboard;
pub mod database;
pub mod models;
pub mod reasoning;
pub mod settings;
pub mod transcription;

pub(crate) trait ResultExt<T> {
    fn str_err(self) -> Result<T, String>;
}

impl<T, E: std::fmt::Display> ResultExt<T> for Result<T, E> {
    fn str_err(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
