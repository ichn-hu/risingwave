// Copyright 2022 Singularity Data
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;

use futures_async_stream::try_stream;

use crate::executor::Message;
use crate::executor_v2::error::StreamExecutorError;
use crate::executor_v2::{ExecutorInfo, MessageStream};

/// Streams wrapped by `epoch_check` will check whether the first message received is a barrier, and
/// the epoch in the barriers are monotonically increasing.
#[try_stream(ok = Message, error = StreamExecutorError)]
pub async fn epoch_check(info: Arc<ExecutorInfo>, input: impl MessageStream) {
    // Epoch number recorded from last barrier message.
    let mut last_epoch = None;

    #[for_await]
    for message in input {
        let message = message?;

        if let Message::Barrier(b) = &message {
            let new_epoch = b.epoch.curr;
            let stale = last_epoch
                .map(|last_epoch| last_epoch > new_epoch)
                .unwrap_or(false);

            if stale {
                panic!(
                    "epoch check failed on {}: last epoch is {:?}, while the epoch of incoming barrier is {}.\nstale barrier: {:?}",
                    info.identity,
                    last_epoch,
                    new_epoch,
                    b
                );
            }
            last_epoch = Some(new_epoch);
        } else if last_epoch.is_none() && !info.identity.contains("BatchQuery") {
            panic!(
                "epoch check failed on {}: the first message must be a barrier",
                info.identity
            )
        }

        yield message;
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use futures::{pin_mut, StreamExt};
    use risingwave_common::array::StreamChunk;

    use super::*;
    use crate::executor_v2::test_utils::MockSource;
    use crate::executor_v2::Executor;

    #[tokio::test]
    async fn test_epoch_ok() {
        let mut source = MockSource::new(Default::default(), vec![]);
        source.push_barrier(100, false);
        source.push_chunks([StreamChunk::default()].into_iter());
        source.push_barrier(114, false);
        source.push_barrier(114, false);
        source.push_barrier(514, false);

        let checked = epoch_check(source.info().into(), source.boxed().execute());
        pin_mut!(checked);

        assert_matches!(checked.next().await.unwrap().unwrap(), Message::Barrier(b) if b.epoch.curr == 100);
        assert_matches!(checked.next().await.unwrap().unwrap(), Message::Chunk(_));
        assert_matches!(checked.next().await.unwrap().unwrap(), Message::Barrier(b) if b.epoch.curr == 114);
        assert_matches!(checked.next().await.unwrap().unwrap(), Message::Barrier(b) if b.epoch.curr == 114);
        assert_matches!(checked.next().await.unwrap().unwrap(), Message::Barrier(b) if b.epoch.curr == 514);
    }

    #[should_panic]
    #[tokio::test]
    async fn test_epoch_bad() {
        let mut source = MockSource::new(Default::default(), vec![]);
        source.push_barrier(100, false);
        source.push_chunks([StreamChunk::default()].into_iter());
        source.push_barrier(514, false);
        source.push_barrier(514, false);
        source.push_barrier(114, false);

        let checked = epoch_check(source.info().into(), source.boxed().execute());
        pin_mut!(checked);

        assert_matches!(checked.next().await.unwrap().unwrap(), Message::Barrier(b) if b.epoch.curr == 100);
        assert_matches!(checked.next().await.unwrap().unwrap(), Message::Chunk(_));
        assert_matches!(checked.next().await.unwrap().unwrap(), Message::Barrier(b) if b.epoch.curr == 514);
        assert_matches!(checked.next().await.unwrap().unwrap(), Message::Barrier(b) if b.epoch.curr == 514);

        checked.next().await.unwrap().unwrap(); // should panic
    }

    #[should_panic]
    #[tokio::test]
    async fn test_epoch_first_not_barrier() {
        let mut source = MockSource::new(Default::default(), vec![]);
        source.push_chunks([StreamChunk::default()].into_iter());
        source.push_barrier(114, false);

        let checked = epoch_check(source.info().into(), source.boxed().execute());
        pin_mut!(checked);

        checked.next().await.unwrap().unwrap(); // should panic
    }

    #[tokio::test]
    async fn test_empty() {
        let source = MockSource::new(Default::default(), vec![]).stop_on_finish(false);
        let checked = epoch_check(source.info().into(), source.boxed().execute());
        pin_mut!(checked);

        assert!(checked.next().await.transpose().unwrap().is_none());
    }
}
