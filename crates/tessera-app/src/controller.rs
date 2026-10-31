use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::mpsc::channel;

use porter_utils::AtomicCancel;

use crate::AssetPreview;

/// Errors returned by an application-owned download request.
#[derive(Debug, Clone)]
pub enum DownloadError {
    Cancelled,
    Failed(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("download cancelled"),
            Self::Failed(error) => formatter.write_str(error),
        }
    }
}

/// Messages sent to the app from background work.
#[derive(Debug, Clone)]
pub enum ControllerMessage {
    /// Requests the given files be loaded by the app.
    LoadFiles(Vec<PathBuf>),
    /// Requests that dropped files be loaded.
    LoadFilesDropped,
    /// Notifies the app of a load result.
    LoadUpdate(Result<(), String>, Option<Vec<PathBuf>>),
    /// Notifies the app of progress being made during an operation.
    ProgressUpdate(bool, u32),
    /// Notifies the app of a preview asset being ready.
    PreviewUpdate(u64, AssetPreview),
    /// Sets a new export directory.
    SetOutputDirectory(PathBuf),
    /// Shows a warning to the user.
    Warning(String),
    /// Begins one application download session.
    DownloadBegin,
    /// Requests a download and replies on the supplied channel.
    DownloadRequest {
        url: String,
        label: String,
        reply: Sender<Result<Vec<u8>, DownloadError>>,
    },
    /// Reports download progress.
    DownloadProgress { percent: u32, bytes_per_sec: u64 },
    /// Ends one application download session.
    DownloadEnd,
}

/// Control the app from anywhere.
#[derive(Clone)]
pub struct Controller {
    channel: Sender<ControllerMessage>,
    receiver: Option<Arc<Mutex<Receiver<ControllerMessage>>>>,
    download_cancel: AtomicCancel,
    headless: bool,
}

impl Controller {
    /// Constructs a new controller.
    pub fn new() -> Self {
        let (tx, rx) = channel();

        Self {
            channel: tx,
            receiver: Some(Arc::new(Mutex::new(rx))),
            download_cancel: AtomicCancel::new(),
            headless: false,
        }
    }

    /// Constructs a controller that sends messages to an application-owned channel.
    pub fn with_channel(channel: Sender<ControllerMessage>) -> Self {
        Self {
            channel,
            receiver: None,
            download_cancel: AtomicCancel::new(),
            headless: false,
        }
    }

    /// Constructs a controller for non-GPUI callers. Download requests run silently.
    pub fn headless() -> Self {
        let mut controller = Self::new();
        controller.headless = true;
        controller
    }

    /// Drains any pending messages. Used by the ui thread.
    pub(crate) fn drain(&self) -> Vec<ControllerMessage> {
        let mut result = Vec::new();

        if let Some(receiver) = &self.receiver
            && let Ok(receiver) = receiver.lock()
        {
            while let Ok(message) = receiver.try_recv() {
                result.push(message);
            }
        }

        result
    }

    /// Sends the given message.
    fn send(&self, message: ControllerMessage) {
        let result = self.channel.send(message);

        debug_assert!(result.is_ok());
    }

    /// Requests the given files be loaded by the app.
    pub fn load_files(&self, files: Vec<PathBuf>) {
        self.send(ControllerMessage::LoadFiles(files));
    }

    /// Requests that dropped files be loaded.
    pub fn load_files_dropped(&self) {
        self.send(ControllerMessage::LoadFilesDropped);
    }

    /// Notifies the app of a load result.
    pub fn load_update(&self, result: Result<(), String>, files: Option<Vec<PathBuf>>) {
        self.send(ControllerMessage::LoadUpdate(result, files));
    }

    /// Notifies the app of progress being made during an operation.
    pub fn progress_update(&self, progress: u32) {
        self.send(ControllerMessage::ProgressUpdate(false, progress));
    }

    /// Notifies the app of progress being finished during an operation.
    pub fn progress_finish(&self) {
        self.send(ControllerMessage::ProgressUpdate(true, 100));
    }

    /// Notifies the app of a preview asset being ready.
    pub fn preview_update(&self, request_id: u64, asset: AssetPreview) {
        self.send(ControllerMessage::PreviewUpdate(request_id, asset));
    }

    /// Notifies the app of the export folder changing.
    pub fn export_folder_update(&self, path: PathBuf) {
        self.send(ControllerMessage::SetOutputDirectory(path));
    }

    /// Sets a new export directory.
    pub fn set_output_directory(&self, path: PathBuf) {
        self.export_folder_update(path);
    }

    /// Shows a warning to the user.
    pub fn warning(&self, message: impl Into<String>) {
        self.send(ControllerMessage::Warning(message.into()));
    }

    pub fn download_begin(&self) {
        self.download_cancel.reset();
        if !self.headless {
            self.send(ControllerMessage::DownloadBegin);
        }
    }

    pub fn download(&self, url: &str, label: &str) -> Result<Vec<u8>, DownloadError> {
        if self.download_cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }

        if self.headless {
            return crate::update::download_bytes(
                url,
                || self.download_cancel.is_cancelled(),
                |_| {},
            )
            .map_err(|error| match error {
                crate::update::UpdateError::Cancelled => DownloadError::Cancelled,
                error => DownloadError::Failed(error.to_string()),
            });
        }

        let (reply, result) = channel();
        self.send(ControllerMessage::DownloadRequest {
            url: url.to_owned(),
            label: label.to_owned(),
            reply,
        });
        result
            .recv()
            .map_err(|_| DownloadError::Failed("download worker stopped".to_owned()))?
    }

    pub fn download_progress(&self, percent: u32, bytes_per_sec: u64) {
        if !self.headless {
            self.send(ControllerMessage::DownloadProgress {
                percent,
                bytes_per_sec,
            });
        }
    }

    pub fn download_cancel(&self) -> AtomicCancel {
        self.download_cancel.clone()
    }

    pub fn download_end(&self) {
        if !self.headless {
            self.send(ControllerMessage::DownloadEnd);
        }
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}
