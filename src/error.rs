use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceError {
    /// Ошибка отправки данных в канал (Producer -> Processor).
    #[error("Channel send error: {0}")]
    ChannelSend(String),
}
