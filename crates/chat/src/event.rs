use crate::Message;

/// A fact produced by a successful chat use case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChatEvent {
    /// A message was stored and is ready for delivery.
    MessagePosted {
        /// The stored message.
        message: Message,
    },
}
