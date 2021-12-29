use std::collections::VecDeque;

use risingwave_storage::memory::MemoryStateStore;
use risingwave_storage::Keyspace;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::executor::*;

#[macro_export]

/// `row_nonnull` builds a `Row` with concrete values.
/// TODO: add macro row!, which requires a new trait `ToScalarValue`.
macro_rules! row_nonnull {
  [$( $value:expr ),*] => {
    {
      use risingwave_common::types::Scalar;
      use risingwave_common::array::Row;
      Row(vec![$(Some($value.to_scalar_value()), )*])
    }
  };
}

pub struct MockSource {
    schema: Schema,
    pk_indices: PkIndices,
    epoch: u64,
    msgs: VecDeque<Message>,
}

impl MockSource {
    pub fn new(schema: Schema, pk_indices: PkIndices) -> Self {
        Self {
            schema,
            pk_indices,
            epoch: 0,
            msgs: VecDeque::default(),
        }
    }

    pub fn with_messages(schema: Schema, pk_indices: PkIndices, msgs: Vec<Message>) -> Self {
        Self {
            schema,
            pk_indices,
            epoch: 0,
            msgs: msgs.into(),
        }
    }

    pub fn with_chunks(schema: Schema, pk_indices: PkIndices, chunks: Vec<StreamChunk>) -> Self {
        Self {
            schema,
            pk_indices,
            epoch: 0,
            msgs: chunks.into_iter().map(Message::Chunk).collect(),
        }
    }

    pub fn push_chunks(&mut self, chunks: impl Iterator<Item = StreamChunk>) {
        self.msgs.extend(chunks.map(Message::Chunk));
    }

    pub fn push_barrier(&mut self, epoch: u64, stop: bool) {
        self.msgs.push_back(Message::Barrier(Barrier {
            epoch,
            mutation: if stop {
                Mutation::Stop
            } else {
                Mutation::Nothing
            },
        }));
    }
}

#[async_trait]
impl Executor for MockSource {
    async fn next(&mut self) -> Result<Message> {
        self.epoch += 1;
        match self.msgs.pop_front() {
            Some(msg) => Ok(msg),
            None => Ok(Message::Barrier(Barrier {
                epoch: self.epoch,
                mutation: Mutation::Stop,
            })),
        }
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }

    fn pk_indices(&self) -> PkIndicesRef {
        &self.pk_indices
    }
}

/// This source takes message from users asynchronously
pub struct MockAsyncSource {
    schema: Schema,
    pk_indices: PkIndices,
    epoch: u64,
    rx: UnboundedReceiver<Message>,
}

impl MockAsyncSource {
    pub fn new(schema: Schema, rx: UnboundedReceiver<Message>) -> Self {
        Self {
            schema,
            pk_indices: vec![],
            rx,
            epoch: 0,
        }
    }

    pub fn with_pk_indices(
        schema: Schema,
        rx: UnboundedReceiver<Message>,
        pk_indices: Vec<usize>,
    ) -> Self {
        Self {
            schema,
            pk_indices,
            rx,
            epoch: 0,
        }
    }

    pub fn push_chunks(
        tx: &mut UnboundedSender<Message>,
        chunks: impl IntoIterator<Item = StreamChunk>,
    ) {
        for chunk in chunks {
            tx.send(Message::Chunk(chunk)).expect("Receiver closed");
        }
    }

    pub fn push_barrier(tx: &mut UnboundedSender<Message>, epoch: u64, stop: bool) {
        tx.send(Message::Barrier(Barrier {
            epoch,
            mutation: if stop {
                Mutation::Stop
            } else {
                Mutation::Nothing
            },
        }))
        .expect("Receiver closed");
    }
}

#[async_trait]
impl Executor for MockAsyncSource {
    async fn next(&mut self) -> Result<Message> {
        self.epoch += 1;
        match self.rx.recv().await {
            Some(msg) => Ok(msg),
            None => Ok(Message::Barrier(Barrier {
                epoch: self.epoch,
                mutation: Mutation::Stop,
            })),
        }
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }

    fn pk_indices(&self) -> PkIndicesRef {
        &self.pk_indices
    }
}

pub fn create_in_memory_keyspace() -> Keyspace<MemoryStateStore> {
    Keyspace::executor_root(MemoryStateStore::new(), 0x2333)
}