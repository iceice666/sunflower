use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime},
};

use axum::extract::ws::{Message, WebSocket};
use chrono::{DateTime, SecondsFormat, Utc};
use futures_util::{SinkExt, StreamExt};
use sunflower_core::{
    NOW_PLAYING_KIND_COMMAND, NowPlayingClientMessage, NowPlayingServerMessage,
    NowPlayingStateResponse,
};
use tokio::{sync::mpsc, time};
use uuid::Uuid;

const SEND_BUFFER: usize = 32;
const PING_PERIOD: Duration = Duration::from_secs(54);

#[derive(Debug)]
pub enum OutboundFrame {
    Text(String),
    Close,
}

struct Connection {
    device_id: String,
    tx: mpsc::Sender<OutboundFrame>,
}

#[derive(Default)]
struct Inner {
    conns: HashMap<Uuid, Connection>,
    state: HashMap<String, NowPlayingStateResponse>,
}

#[derive(Default)]
pub struct NowPlayingHub {
    inner: Mutex<Inner>,
}

pub struct Registration {
    pub id: Uuid,
    pub rx: mpsc::Receiver<OutboundFrame>,
    pub initial_frames: Vec<String>,
}

impl NowPlayingHub {
    fn lock_inner(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn register(&self, device_id: String) -> Registration {
        let (tx, rx) = mpsc::channel(SEND_BUFFER);
        let id = Uuid::new_v4();
        let mut inner = self.lock_inner();
        let initial_frames = inner
            .state
            .values()
            .filter(|state| state.device_id != device_id)
            .filter_map(|state| {
                serde_json::to_string(&NowPlayingClientMessage::from_state(state)).ok()
            })
            .collect();
        inner.conns.insert(id, Connection { device_id, tx });
        Registration {
            id,
            rx,
            initial_frames,
        }
    }

    pub fn unregister(&self, id: Uuid) {
        self.lock_inner().conns.remove(&id);
    }

    pub fn snapshot(&self) -> Vec<NowPlayingStateResponse> {
        self.lock_inner().state.values().cloned().collect()
    }

    pub fn on_client_message(
        &self,
        sender_id: Uuid,
        device_id: &str,
        message: NowPlayingClientMessage,
    ) {
        let payload = match serde_json::to_string(&message) {
            Ok(payload) => payload,
            Err(_) => return,
        };
        let targets = {
            let mut inner = self.lock_inner();
            inner.state.insert(
                device_id.to_string(),
                NowPlayingStateResponse {
                    device_id: device_id.to_string(),
                    queue_id: message.queue_id,
                    media_id: message.media_id,
                    title: message.title,
                    artist: message.artist,
                    position_ms: message.position_ms,
                    duration_ms: message.duration_ms,
                    is_playing: message.is_playing,
                    updated_at: rfc3339_seconds(SystemTime::now()),
                },
            );
            inner
                .conns
                .iter()
                .filter(|(id, _)| **id != sender_id)
                .map(|(_, conn)| conn.tx.clone())
                .collect::<Vec<_>>()
        };
        for tx in targets {
            let _ = tx.try_send(OutboundFrame::Text(payload.clone()));
        }
    }

    pub fn send_command(&self, device_id: &str, command: &str) -> i32 {
        let payload = match serde_json::to_string(&NowPlayingServerMessage {
            kind: NOW_PLAYING_KIND_COMMAND.to_string(),
            command: command.to_string(),
        }) {
            Ok(payload) => payload,
            Err(_) => return 0,
        };
        let targets = {
            let inner = self.lock_inner();
            inner
                .conns
                .values()
                .filter(|conn| conn.device_id == device_id)
                .map(|conn| conn.tx.clone())
                .collect::<Vec<_>>()
        };
        let mut delivered = 0;
        for tx in targets {
            if tx.try_send(OutboundFrame::Text(payload.clone())).is_ok() {
                delivered += 1;
            }
        }
        delivered
    }

    pub fn disconnect_device(&self, device_id: &str) -> i32 {
        let targets = {
            let inner = self.lock_inner();
            inner
                .conns
                .values()
                .filter(|conn| conn.device_id == device_id)
                .map(|conn| conn.tx.clone())
                .collect::<Vec<_>>()
        };
        let count = targets.len() as i32;
        for tx in targets {
            let _ = tx.try_send(OutboundFrame::Close);
        }
        count
    }
}

pub async fn serve_socket(
    socket: WebSocket,
    hub: std::sync::Arc<NowPlayingHub>,
    device_id: String,
) {
    let Registration {
        id,
        mut rx,
        initial_frames,
    } = hub.register(device_id.clone());

    let (mut sender, mut receiver) = socket.split();
    for frame in initial_frames {
        if sender.send(Message::Text(frame)).await.is_err() {
            hub.unregister(id);
            return;
        }
    }

    let writer_hub = hub.clone();
    let writer = tokio::spawn(async move {
        let mut ping = time::interval(PING_PERIOD);
        ping.tick().await;
        loop {
            tokio::select! {
                maybe_frame = rx.recv() => {
                    match maybe_frame {
                        Some(OutboundFrame::Text(frame)) => {
                            if sender.send(Message::Text(frame)).await.is_err() {
                                break;
                            }
                        }
                        Some(OutboundFrame::Close) => {
                            let _ = sender.send(Message::Close(None)).await;
                            break;
                        }
                        None => break,
                    }
                }
                _ = ping.tick() => {
                    if sender.send(Message::Ping(Vec::new())).await.is_err() {
                        break;
                    }
                }
            }
        }
        writer_hub.unregister(id);
    });

    while let Some(result) = receiver.next().await {
        let Ok(message) = result else {
            break;
        };
        match message {
            Message::Text(raw) => {
                if let Ok(message) = NowPlayingClientMessage::parse_json(&raw) {
                    hub.on_client_message(id, &device_id, message);
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    hub.unregister(id);
    writer.abort();
}

fn rfc3339_seconds(time: SystemTime) -> String {
    let time: DateTime<Utc> = time.into();
    time.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sunflower_core::{
        NOW_PLAYING_CMD_PAUSE, NOW_PLAYING_KIND_TICK, NowPlayingClientMessage,
        NowPlayingServerMessage,
    };

    #[test]
    fn command_message_matches_go_protocol_json() {
        let raw = serde_json::to_string(&NowPlayingServerMessage {
            kind: NOW_PLAYING_KIND_COMMAND.to_string(),
            command: NOW_PLAYING_CMD_PAUSE.to_string(),
        })
        .unwrap();
        assert_eq!(raw, r#"{"type":"command","command":"pause"}"#);
    }

    #[test]
    fn client_message_decodes_unknown_fields_like_go() {
        let message = NowPlayingClientMessage::parse_json(
            r#"{"type":"tick","media_id":"x","extra_field":42,"is_playing":true}"#,
        )
        .unwrap();
        assert_eq!(message.kind, NOW_PLAYING_KIND_TICK);
        assert_eq!(message.media_id, "x");
        assert!(message.is_playing);
    }

    #[test]
    fn hub_recovers_from_poisoned_lock() {
        let hub = NowPlayingHub::default();
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = hub.inner.lock().unwrap();
            panic!("poison now-playing hub");
        }));
        assert!(poisoned.is_err());

        let registration = hub.register("playerDev".into());

        assert!(registration.initial_frames.is_empty());
        assert!(hub.snapshot().is_empty());
        assert_eq!(hub.send_command("playerDev", NOW_PLAYING_CMD_PAUSE), 1);
        assert_eq!(hub.disconnect_device("playerDev"), 1);
        hub.unregister(registration.id);
    }

    #[tokio::test]
    async fn hub_broadcasts_snapshot_and_commands_like_go() {
        let hub = NowPlayingHub::default();
        let player = hub.register("playerDev".into());
        let observer = hub.register("observerDev".into());
        let mut player_rx = player.rx;
        let mut observer_rx = observer.rx;

        hub.on_client_message(
            player.id,
            "playerDev",
            NowPlayingClientMessage {
                kind: NOW_PLAYING_KIND_TICK.to_string(),
                queue_id: String::new(),
                media_id: "yt:abc".into(),
                title: String::new(),
                artist: String::new(),
                position_ms: 5000,
                duration_ms: 0,
                is_playing: true,
                shuffle: false,
                repeat: String::new(),
            },
        );

        let observed = observer_rx.recv().await.expect("observer broadcast");
        let OutboundFrame::Text(observed) = observed else {
            panic!("expected text frame");
        };
        let observed = NowPlayingClientMessage::parse_json(&observed).unwrap();
        assert_eq!(observed.media_id, "yt:abc");
        assert_eq!(observed.position_ms, 5000);
        assert!(player_rx.try_recv().is_err());
        assert!(
            hub.snapshot()
                .iter()
                .any(|state| state.device_id == "playerDev" && state.media_id == "yt:abc")
        );

        assert_eq!(hub.send_command("playerDev", NOW_PLAYING_CMD_PAUSE), 1);
        let command = player_rx.recv().await.expect("player command");
        let OutboundFrame::Text(command) = command else {
            panic!("expected command frame");
        };
        assert_eq!(command, r#"{"type":"command","command":"pause"}"#);
    }
}
