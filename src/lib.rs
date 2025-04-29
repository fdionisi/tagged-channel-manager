mod channel_id;
mod error;

use std::{
    collections::HashMap,
    fmt,
    hash::Hash,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures::{
    channel::mpsc::{self, Receiver, SendError, Sender},
    executor::block_on,
    lock::Mutex,
    SinkExt, Stream,
};

use crate::channel_id::ChannelId;
pub use error::Error;

pub struct TaggedChannelManager<T, M> {
    inner: Arc<Mutex<Inner<T, M>>>,
}

#[derive(Default)]
struct Inner<T, M> {
    last_channel_id: ChannelId,
    channels: HashMap<ChannelId, Channel<T, M>>,
    channel_id_by_tag: HashMap<T, ChannelId>,
}

impl<T, M> TaggedChannelManager<T, M>
where
    T: Clone + Eq + Hash + fmt::Display,
{
    pub fn new() -> Self {
        TaggedChannelManager::default()
    }

    pub async fn open_channel(&self, tag: T) -> GuardedReceiver<T, M> {
        let (tx, rx) = mpsc::channel(1);

        let channel = Channel::new(tag.clone(), tx);

        let mut inner = self.inner.lock().await;

        let channel_id = inner.last_channel_id;
        inner.last_channel_id += 1;

        inner.channels.insert(channel_id, channel);
        inner.channel_id_by_tag.insert(tag, channel_id);

        GuardedReceiver {
            rx,
            guard: ChannelGuard {
                channel_id,
                manager: self.clone(),
            },
        }
    }

    pub async fn send_message(&self, tag: T, message: M) -> Result<(), Error> {
        let inner = self.inner.lock().await;
        let Some(channel_id) = inner.channel_id_by_tag.get(&tag) else {
            return Err(Error::TagNotFound(tag.to_string()));
        };

        let Some(channel) = inner.channels.get(channel_id) else {
            return Err(Error::ChannelNotFound(*channel_id));
        };

        channel.send(message).await?;

        Ok(())
    }
}

impl<T, M> Default for TaggedChannelManager<T, M> {
    fn default() -> Self {
        TaggedChannelManager {
            inner: Arc::new(Mutex::new(Inner {
                last_channel_id: ChannelId::default(),
                channels: HashMap::default(),
                channel_id_by_tag: HashMap::default(),
            })),
        }
    }
}

impl<M, T> Clone for TaggedChannelManager<M, T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[derive(Debug)]
pub struct Channel<T, M> {
    tag: T,
    tx: Sender<M>,
}

impl<T, M> Channel<T, M> {
    pub fn new(tag: T, tx: Sender<M>) -> Self {
        Channel {
            tag: tag.into(),
            tx,
        }
    }

    pub async fn send(&self, message: M) -> Result<(), SendError> {
        self.tx.clone().send(message).await
    }
}

pub struct ChannelGuard<T, M>
where
    T: Clone + Eq + Hash,
{
    channel_id: ChannelId,
    manager: TaggedChannelManager<T, M>,
}

impl<T, M> Drop for ChannelGuard<T, M>
where
    T: Clone + Eq + Hash,
{
    fn drop(&mut self) {
        let channel_id = self.channel_id.clone();
        let manager = self.manager.clone();
        block_on(async move {
            let mut inner = manager.inner.lock().await;
            let Some(channel) = inner.channels.remove(&channel_id) else {
                debug_assert!(false, "tag is already not present in channels");
                return;
            };

            let Some(_) = inner.channel_id_by_tag.remove(&channel.tag) else {
                debug_assert!(false, "tag is already not present in channel_id_by_tag");
                return;
            };
        });
    }
}

pub struct GuardedReceiver<T, M>
where
    T: Clone + Eq + Hash,
{
    rx: Receiver<M>,
    #[allow(unused)]
    guard: ChannelGuard<T, M>,
}

impl<T, M> Stream for GuardedReceiver<T, M>
where
    T: Clone + Eq + Hash,
{
    type Item = M;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use futures::StreamExt;

    use super::*;

    #[test]
    fn it_can_instantiate() {
        let _: TaggedChannelManager<String, String> = TaggedChannelManager::new();
    }

    #[tokio::test]
    async fn it_can_create_and_remember_new_channel_with_specific_tag() {
        let manager: TaggedChannelManager<&str, u8> = TaggedChannelManager::new();
        let tag = "test";

        let receiver = manager.open_channel(tag).await;

        assert_eq!(
            {
                let inner = manager.inner.lock().await;

                inner
                    .channels
                    .get(&receiver.guard.channel_id)
                    .map(|channel| channel.tag)
            },
            Some("test".into())
        );
    }

    #[tokio::test]
    async fn it_can_create_multiple_tag_channel_pairs() {
        let manager: TaggedChannelManager<&str, u8> = TaggedChannelManager::new();
        let tag1 = "test1";
        let tag2 = "test2";

        let receiver1 = manager.open_channel(tag1).await;
        let receiver2 = manager.open_channel(tag2).await;

        assert_eq!(
            {
                let inner = manager.inner.lock().await;

                inner
                    .channels
                    .get(&receiver1.guard.channel_id)
                    .map(|channel| channel.tag)
            },
            Some("test1".into())
        );
        assert_eq!(
            {
                let inner = manager.inner.lock().await;

                inner
                    .channels
                    .get(&receiver2.guard.channel_id)
                    .map(|channel| channel.tag)
            },
            Some("test2".into())
        );
    }

    #[tokio::test]
    async fn it_can_send_messages_via_tag() {
        let manager = TaggedChannelManager::new();
        let tag = "test";
        let _receiver = manager.open_channel(tag).await;

        let result = manager.send_message(tag, "message").await;

        assert_eq!(result, Ok(()))
    }

    #[tokio::test]
    async fn it_fails_sending_messages_via_unexistiting_tag() {
        let manager = TaggedChannelManager::new();
        let tag = "test";

        let result = manager.send_message(tag, "message").await;

        assert_eq!(result, Err(Error::TagNotFound(tag.into())))
    }

    #[tokio::test]
    async fn it_sends_and_receive_messages() {
        let manager = TaggedChannelManager::new();
        let tag = "test";
        let message = "message";
        let mut receiver = manager.open_channel(tag).await;

        let _ = manager.send_message(tag, message).await;

        assert_eq!(receiver.next().await, Some(message))
    }

    #[tokio::test]
    async fn it_clean_everything_after_drop() {
        let manager: TaggedChannelManager<&str, u8> = TaggedChannelManager::new();
        let tag = "test";
        let receiver = manager.open_channel(tag).await;

        drop(receiver);

        let inner = manager.inner.lock().await;

        assert_eq!(inner.channel_id_by_tag.len(), 0);
        assert_eq!(inner.channels.len(), 0);
    }

    #[tokio::test]
    async fn it_can_move_through_threads() {
        let manager: TaggedChannelManager<&str, u8> = TaggedChannelManager::new();
        let tag = "test";
        let _receiver = manager.open_channel(tag).await;

        thread::spawn({
            let manager = manager.clone();
            move || async move {
                let tag = "test2";
                let _receiver = manager.open_channel(tag).await;
            }
        })
        .join()
        .unwrap()
        .await;
    }
}
