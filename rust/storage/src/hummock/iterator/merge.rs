use super::variants::FORWARD;
use crate::hummock::iterator::merge_inner::MergeIteratorInner;

pub type MergeIterator = MergeIteratorInner<FORWARD>;

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use itertools::Itertools;

    use super::*;
    use crate::hummock::iterator::test_utils::{
        default_builder_opt_for_test, gen_test_sstable, iterator_test_key_of, test_key,
        test_value_of, TestIteratorBuilder, TEST_KEYS_COUNT,
    };
    use crate::hummock::iterator::{BoxedHummockIterator, HummockIterator};
    use crate::hummock::sstable::SSTableIterator;

    #[tokio::test]
    async fn test_merge_basic() {
        let (iters, validators): (Vec<_>, Vec<_>) = (0..3)
            .map(|iter_id| {
                TestIteratorBuilder::<FORWARD>::default()
                    .id(0)
                    .map_key(move |id, x| iterator_test_key_of(id, x * 3 + (iter_id as usize) + 1))
                    .map_value(move |id, x| test_value_of(id, x * 3 + (iter_id as usize) + 1))
                    .finish()
            })
            .unzip();

        let iters: Vec<BoxedHummockIterator> = iters
            .into_iter()
            .map(|x| Box::new(x) as BoxedHummockIterator)
            .collect_vec();

        let mut mi = MergeIterator::new(iters);
        let mut i = 0;
        mi.rewind().await.unwrap();
        while mi.is_valid() {
            let key = mi.key();
            let val = mi.value();
            validators[i % 3].assert_key(i / 3, key);
            validators[i % 3].assert_hummock_value(i / 3, val);
            i += 1;
            mi.next().await.unwrap();
            if i == TEST_KEYS_COUNT * 3 {
                assert!(!mi.is_valid());
                break;
            }
        }
        assert!(i >= TEST_KEYS_COUNT);
    }

    #[tokio::test]
    async fn test_merge_seek() {
        let (iters, validators): (Vec<_>, Vec<_>) = (0..3)
            .map(|iter_id| {
                TestIteratorBuilder::<FORWARD>::default()
                    .id(0)
                    .total(20)
                    .map_key(move |id, x| iterator_test_key_of(id, x * 3 + (iter_id as usize)))
                    .finish()
            })
            .unzip();
        let iters: Vec<BoxedHummockIterator> = iters
            .into_iter()
            .map(|x| Box::new(x) as BoxedHummockIterator)
            .collect_vec();

        let mut mi = MergeIterator::new(iters);
        let test_validator = &validators[2];

        // right edge case
        mi.seek(test_key!(test_validator, 3 * TEST_KEYS_COUNT))
            .await
            .unwrap();
        assert!(!mi.is_valid());

        // normal case
        mi.seek(test_key!(test_validator, 4)).await.unwrap();
        let k = mi.key();
        let v = mi.value();
        test_validator.assert_hummock_value(4, v);
        test_validator.assert_key(4, k);

        mi.seek(test_key!(test_validator, 17)).await.unwrap();
        let k = mi.key();
        let v = mi.value();
        test_validator.assert_hummock_value(17, v);
        test_validator.assert_key(17, k);

        // left edge case
        mi.seek(test_key!(test_validator, 0)).await.unwrap();
        let k = mi.key();
        let v = mi.value();
        test_validator.assert_hummock_value(0, v);
        test_validator.assert_key(0, k);
    }

    #[tokio::test]
    async fn test_merge_invalidate_reset() {
        let table0 = gen_test_sstable(0, default_builder_opt_for_test()).await;
        let table1 = gen_test_sstable(1, default_builder_opt_for_test()).await;
        let iters: Vec<BoxedHummockIterator> = vec![
            Box::new(SSTableIterator::new(Arc::new(table0))),
            Box::new(SSTableIterator::new(Arc::new(table1))),
        ];

        let mut mi = MergeIterator::new(iters);

        mi.rewind().await.unwrap();
        let mut count = 0;
        while mi.is_valid() {
            count += 1;
            mi.next().await.unwrap();
        }
        assert_eq!(count, TEST_KEYS_COUNT * 2);

        mi.rewind().await.unwrap();
        let mut count = 0;
        while mi.is_valid() {
            count += 1;
            mi.next().await.unwrap();
        }
        assert_eq!(count, TEST_KEYS_COUNT * 2);
    }
}