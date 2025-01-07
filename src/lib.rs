use std::{
    collections::HashMap,
    hash::Hash,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures::{
    channel::mpsc::{self, Receiver, Sender},
    executor::block_on,
    lock::Mutex,
    SinkExt, Stream,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelId(u64);

pub struct TaggedChannelManager<M, T>(Arc<Mutex<ChannelsInner<M, T>>>);

impl<M, T> Clone for TaggedChannelManager<M, T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub struct ChannelsInner<M, T> {
    last_id: ChannelId,
    channels: HashMap<ChannelId, Channel<M, T>>,
    tags: HashMap<T, ChannelId>,
}

struct Channel<M, T> {
    tx: Sender<M>,
    tag: T,
}

pub struct ChannelGuard<M, T>
where
    T: Clone + Eq + Hash,
{
    channel_id: ChannelId,
    manager: TaggedChannelManager<M, T>,
}

pub struct GuardedReceiver<M, T>
where
    T: Clone + Eq + Hash,
{
    rx: Receiver<M>,
    #[allow(dead_code)]
    guard: ChannelGuard<M, T>,
}

impl<M, T> TaggedChannelManager<M, T>
where
    T: Clone + Eq + Hash,
{
    pub fn new() -> Self {
        Default::default()
    }

    pub async fn create_channel(&self, tag: T) -> GuardedReceiver<M, T> {
        let (tx, rx) = mpsc::channel::<M>(1);
        let mut inner = self.0.lock().await;
        let channel_id = ChannelId(inner.last_id.0.wrapping_add(1));
        inner.channels.insert(
            channel_id,
            Channel {
                tx,
                tag: tag.clone(),
            },
        );
        inner.tags.insert(tag, channel_id);
        inner.last_id = channel_id;

        GuardedReceiver {
            rx,
            guard: ChannelGuard {
                channel_id,
                manager: self.clone(),
            },
        }
    }

    pub async fn send_message(&self, tag: &T, message: M) {
        let inner = self.0.lock().await;
        if let Some(channel_id) = inner.tags.get(tag) {
            if let Some(channel) = inner.channels.get(channel_id) {
                let _ = channel.tx.clone().send(message).await;
            }
        }
    }

    async fn remove_channel(&self, channel_id: &ChannelId) {
        let mut inner = self.0.lock().await;
        if let Some(channel) = inner.channels.remove(channel_id) {
            inner.tags.remove(&channel.tag);
        }
    }
}

impl<M, T> Default for TaggedChannelManager<M, T> {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(ChannelsInner {
            last_id: ChannelId(0),
            channels: HashMap::new(),
            tags: HashMap::new(),
        })))
    }
}

impl<M, T> Drop for ChannelGuard<M, T>
where
    T: Clone + Eq + Hash,
{
    fn drop(&mut self) {
        let manager = self.manager.clone();
        let channel_id = self.channel_id;
        block_on(async move {
            manager.remove_channel(&channel_id).await;
        });
    }
}

impl<M, T> Stream for GuardedReceiver<M, T>
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
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_create_channel() {
        let manager = TaggedChannelManager::<String, u32>::new();
        let _receiver = manager.create_channel(1).await;

        let inner = manager.0.lock().await;
        assert_eq!(inner.channels.len(), 1);
        assert_eq!(inner.tags.len(), 1);
    }

    #[tokio::test]
    async fn test_send_message() {
        let manager = TaggedChannelManager::<String, u32>::new();
        let mut receiver = manager.create_channel(1).await;

        manager.send_message(&1, "Hello".to_string()).await;

        let message = receiver.next().await;
        assert_eq!(message, Some("Hello".to_string()));
    }

    #[tokio::test]
    async fn test_remove_channel() {
        let manager = TaggedChannelManager::<String, u32>::new();
        let receiver = manager.create_channel(1).await;

        drop(receiver);

        let inner = manager.0.lock().await;
        assert_eq!(inner.channels.len(), 0);
        assert_eq!(inner.tags.len(), 0);
    }

    #[tokio::test]
    async fn test_multiple_channels() {
        let manager = TaggedChannelManager::<String, u32>::new();
        let mut receiver1 = manager.create_channel(1).await;
        let mut receiver2 = manager.create_channel(2).await;

        manager.send_message(&1, "Hello".to_string()).await;
        manager.send_message(&2, "World".to_string()).await;

        assert_eq!(receiver1.next().await, Some("Hello".to_string()));
        assert_eq!(receiver2.next().await, Some("World".to_string()));
    }
}
