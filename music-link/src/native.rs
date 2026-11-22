use crate::*;
use hearsay::{Client, ClientSettings, Route};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

/// Latest-value output, bounded command input, and a dedicated async runtime.
/// Drop terminates the worker; a missing hub cannot block the music producer.
pub struct Link {
    latest: Arc<Mutex<Option<Snapshot>>>,
    audio: Arc<Mutex<Option<AudioFrame>>>,
    commands: mpsc::Receiver<Command>,
    responses: mpsc::SyncSender<Response>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Link {
    pub fn connect() -> std::io::Result<Self> {
        let latest = Arc::new(Mutex::new(None::<Snapshot>));
        let shared = latest.clone();
        let audio = Arc::new(Mutex::new(None::<AudioFrame>));
        let shared_audio = audio.clone();
        let (commands_tx, commands) = mpsc::sync_channel(64);
        let (responses, responses_rx) = mpsc::sync_channel::<Response>(64);
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        std::thread::Builder::new().name("music-link".into()).spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            runtime.block_on(async move {
                let client = Client::new("rustel-music", ClientSettings { max_connection_attempts: None, ..Default::default() });
                tokio::select! {
                    _ = &mut stopped => return,
                    result = client.connect("127.0.0.1:9612") => { if result.is_err() { return; } }
                }
                let _ = client.subscribe(&[COMMAND_TOPIC]).await;
                let mut timer = tokio::time::interval(Duration::from_millis(33));
                loop {
                    tokio::select! {
                        _ = &mut stopped => break,
                        _ = timer.tick() => {
                            let audio = shared_audio.lock().ok().and_then(|mut guard| guard.take());
                            if let Some(audio) = audio {
                                let _ = client.publish_json(AUDIO_TOPIC, &audio, Route::Global).await;
                            }
                            let snapshot = shared.lock().ok().and_then(|mut guard| guard.take());
                            if let Some(snapshot) = snapshot {
                                let _ = client.publish_json(SNAPSHOT_TOPIC, &snapshot, Route::Global).await;
                            }
                            while let Ok(response) = responses_rx.try_recv() {
                                let _ = client.publish_json(RESPONSE_TOPIC, &response, Route::Global).await;
                            }
                        }
                        message = client.next_message() => {
                            if let Some(message) = message {
                                if let Ok(command) = message.read_json::<Command>() {
                                    let _ = commands_tx.try_send(command);
                                }
                            } else { break; }
                        }
                    }
                }
            });
        })?;
        Ok(Self {
            latest,
            audio,
            commands,
            responses,
            stop: Some(stop),
        })
    }

    pub fn publish(&self, snapshot: Snapshot) {
        if let Ok(mut latest) = self.latest.try_lock() {
            *latest = Some(snapshot);
        }
    }
    pub fn commands(&self) -> impl Iterator<Item = Command> + '_ {
        self.commands.try_iter().take(64)
    }
    pub fn publish_audio(&self, frame: AudioFrame) {
        if let Ok(mut latest) = self.audio.try_lock() {
            *latest = Some(frame);
        }
    }
    pub fn respond(&self, response: Response) {
        let _ = self.responses.try_send(response);
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}
