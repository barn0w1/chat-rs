/// The entry point for chat use cases.
#[derive(Clone, Debug)]
pub struct Chat<S> {
    store: S,
}

impl<S> Chat<S> {
    /// Creates a chat application backed by `store`.
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    pub(crate) const fn store(&self) -> &S {
        &self.store
    }
}
