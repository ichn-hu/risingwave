use std::sync::Arc;

use futures::channel::mpsc::unbounded;
use risingwave_batch::executor::{
    CreateTableExecutor, Executor as BatchExecutor, InsertExecutor, SeqScanExecutor,
};
use risingwave_common::array::column::Column;
use risingwave_common::array::{Array, DataChunk, F64Array};
use risingwave_common::array_nonnull;
use risingwave_common::catalog::{Field, Schema, TableId};
use risingwave_common::error::Result;
use risingwave_common::types::{Float64Type, Int64Type, IntoOrdered};
use risingwave_common::util::sort_util::OrderType;
use risingwave_source::{MemSourceManager, SourceManager};
use risingwave_storage::memory::MemoryStateStore;
use risingwave_storage::table::{SimpleTableManager, TableManager};
use risingwave_storage::{Keyspace, StateStoreImpl, TableColumnDesc};
use risingwave_stream::executor::{
    Barrier, Executor as StreamExecutor, MaterializeExecutor, Message, Mutation, PkIndices,
    StreamSourceExecutor,
};

struct SingleChunkExecutor {
    chunk: Option<DataChunk>,
    schema: Schema,
}

impl SingleChunkExecutor {
    pub fn new(chunk: DataChunk, schema: Schema) -> Self {
        Self {
            chunk: Some(chunk),
            schema,
        }
    }
}

#[async_trait::async_trait]
impl BatchExecutor for SingleChunkExecutor {
    async fn open(&mut self) -> Result<()> {
        Ok(())
    }

    async fn next(&mut self) -> Result<Option<DataChunk>> {
        Ok(self.chunk.take())
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }
}

/// This test checks whether batch task and streaming task work together for `TableV2` creation and
/// materialization.
#[tokio::test]
async fn test_table_v2_materialize() -> Result<()> {
    let memory_state_store = MemoryStateStore::new();
    let store = StateStoreImpl::MemoryStateStore(memory_state_store.clone());
    let source_manager = Arc::new(MemSourceManager::new());
    let table_manager = Arc::new(SimpleTableManager::new(store));
    let table_id = TableId::default();
    let column_descs = vec![
        TableColumnDesc {
            // row id column
            data_type: Arc::new(Int64Type::new(false)),
            column_id: 0,
        },
        TableColumnDesc {
            // data column
            data_type: Arc::new(Float64Type::new(false)),
            column_id: 233,
        },
    ];

    // Create table v2 using `CreateTableExecutor`
    let mut create_table = CreateTableExecutor::new_v2(
        table_id.clone(),
        table_manager.clone(),
        source_manager.clone(),
        column_descs,
    );
    // Execute
    create_table.open().await?;
    create_table.next().await?;
    create_table.close().await?;

    // Ensure the source exists
    let source_desc = source_manager.get_source(&table_id)?;
    let get_schema = |column_ids: &[i32]| {
        let mut fields = Vec::with_capacity(column_ids.len());
        for &column_id in column_ids {
            let column_desc = source_desc
                .columns
                .iter()
                .find(|c| c.column_id == column_id)
                .unwrap();
            fields.push(Field::new(column_desc.data_type.clone()));
        }
        Schema::new(fields)
    };

    // Create a `StreamSourceExecutor` to read the changes
    let all_column_ids = vec![0, 233];
    let all_schema = get_schema(&all_column_ids);
    let (barrier_tx, barrier_rx) = unbounded();
    let stream_source = StreamSourceExecutor::new(
        table_id.clone(),
        source_desc.clone(),
        all_column_ids.clone(),
        all_schema.clone(),
        PkIndices::from([0]),
        barrier_rx,
    )?;

    // Create a `Materialize` (`Materialize`) to write the changes to storage
    let keyspace = Keyspace::table_root(memory_state_store, &table_id);
    let mut materialize = MaterializeExecutor::new(
        Box::new(stream_source),
        keyspace.clone(),
        all_schema.clone(),
        vec![0],
        vec![OrderType::Ascending],
    );

    // Add some data using `InsertExecutor`
    let columns = vec![Column::new(Arc::new(
        array_nonnull! { F64Array, [1.14, 5.14] }.into(),
    ))];
    let chunk = DataChunk::builder().columns(columns.clone()).build();
    let insert_inner = SingleChunkExecutor::new(chunk, all_schema);
    let mut insert = InsertExecutor::new(
        table_id.clone(),
        source_manager.clone(),
        Box::new(insert_inner),
    );

    insert.open().await?;
    insert.next().await?;
    insert.close().await?;

    // Since we have not polled `Materialize`, we cannot scan anything from this table
    let table = table_manager.get_table(&table_id)?;
    let data_column_ids = vec![233];
    let data_schema = get_schema(&data_column_ids);

    let mut scan =
        SeqScanExecutor::new(table.clone(), data_column_ids.clone(), data_schema.clone());
    scan.open().await?;
    let c = scan.next().await?.unwrap();
    assert_eq!(c.cardinality(), 0);

    // Poll `Materialize`, should output the same stream chunk
    let message = materialize.next().await?;
    match message {
        Message::Chunk(c) => {
            let col_row_id = c.columns()[0].array_ref().as_int64();
            assert_eq!(col_row_id.value_at(0).unwrap(), 0);
            assert_eq!(col_row_id.value_at(1).unwrap(), 1);

            let col_data = c.columns()[1].array_ref().as_float64();
            assert_eq!(col_data.value_at(0).unwrap(), 1.14.into_ordered());
            assert_eq!(col_data.value_at(1).unwrap(), 5.14.into_ordered());
        }
        Message::Barrier(_) => panic!(),
    }

    // Send a barrier and poll again, should write changes to storage
    barrier_tx
        .unbounded_send(Message::Barrier(Barrier {
            epoch: 1919,
            mutation: Mutation::Nothing,
        }))
        .unwrap();

    assert!(matches!(
        materialize.next().await?,
        Message::Barrier(Barrier { epoch: 1919, .. })
    ));

    // Scan the table again, we are able to get the data now!
    let mut scan =
        SeqScanExecutor::new(table.clone(), data_column_ids.clone(), data_schema.clone());
    scan.open().await?;
    let c = scan.next().await?.unwrap();
    let col_data = c.columns()[0].array_ref().as_float64();
    assert_eq!(col_data.value_at(0).unwrap(), 1.14.into_ordered());
    assert_eq!(col_data.value_at(1).unwrap(), 5.14.into_ordered());

    Ok(())
}