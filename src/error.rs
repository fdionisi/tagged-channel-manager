use futures::channel::mpsc::SendError;

use crate::channel_id::ChannelId;

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("Channel not found for tag {0:?}")]
    TagNotFound(String),
    #[error("Channel not found for ID {0:?}")]
    ChannelNotFound(ChannelId),
    #[error("Channel send error: {0}")]
    ChannelSendError(#[from] SendError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_display_channel_not_found() {
        let error = Error::TagNotFound("123".into());
        assert_eq!(error.to_string(), "Channel not found for tag \"123\"");
    }

    #[test]
    fn it_display_channel_id_not_found() {
        let error = Error::ChannelNotFound(123.into());
        assert_eq!(error.to_string(), "Channel not found for ID ChannelId(123)");
    }

    #[test]
    fn it_can_compare_two_errors() {
        let error1 = Error::TagNotFound("123".into());
        let error2 = Error::TagNotFound("123".into());
        assert_eq!(error1, error2);
    }
}
