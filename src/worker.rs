use crate::message::MessagePayload;
use crate::{ChannelReceiver, ChannelSender};
use tokio::task::JoinHandle;

/// Maximum number of retries for failed requests
const MAX_RETRIES: usize = 10;

/// Provides a background worker task that sends the messages generated by the
/// layer.
pub(crate) async fn worker(mut rx: ChannelReceiver) {
    let client = reqwest::Client::new();
    while let Some(message) = rx.recv().await {
        match message {
            WorkerMessage::Data(payload) => {
                let webhook_url = payload.webhook_url().to_string();
                let payload =
                    serde_json::to_string(&payload).expect("failed to deserialize discord payload, this is a bug");

                let mut retries = 0;
                while retries < MAX_RETRIES {
                    match client
                        .post(webhook_url.clone())
                        .header("Content-Type", "application/json")
                        .body(payload.clone())
                        .send()
                        .await
                    {
                        Ok(res) => {
                            let res_text = res.text().await.unwrap();
                            break; // Success, break out of the retry loop
                        }
                        Err(e) => {}
                    };

                    // Exponential backoff - increase the delay between retries
                    let delay_ms = 2u64.pow(retries as u32) * 100;
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    retries += 1;
                }
            }
            WorkerMessage::Shutdown => {
                break;
            }
        }
    }
}

/// This worker manages a background async task that schedules the network requests to send traces
/// to the Discord on the running tokio runtime.
///
/// Ensure to invoke `.startup()` before, and `.teardown()` after, your application code runs. This
/// is required to ensure proper initialization and shutdown.
///
/// `tracing-layer-discord` synchronously generates payloads to send to the Discord API using the
/// tracing events from the global subscriber. However, all network requests are offloaded onto
/// an unbuffered channel and processed by a provided future acting as an asynchronous worker.
pub struct BackgroundWorker {
    pub(crate) sender: ChannelSender,
    pub(crate) handle: JoinHandle<()>,
}

impl BackgroundWorker {
    /// Initiate the worker's shutdown sequence.
    ///
    /// Without invoking`.teardown()`, your application may exit before all Discord messages can be
    /// sent.
    pub async fn shutdown(self) {
        self.sender.send(WorkerMessage::Shutdown).unwrap();
        self.handle.await.unwrap();
    }
}

#[derive(Debug)]
pub(crate) enum WorkerMessage {
    Data(MessagePayload),
    Shutdown,
}
