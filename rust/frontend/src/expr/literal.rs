use risingwave_common::types::Datum;
use risingwave_pb::data::DataType;
use risingwave_pb::expr::expr_node;

use super::BoundExpr;
#[derive(Clone, Debug)]
pub struct BoundLiteral {
    #[allow(dead_code)]
    data: Datum,
    data_type: DataType,
}
impl BoundLiteral {
    pub fn new(data: Datum, data_type: DataType) -> Self {
        BoundLiteral { data, data_type }
    }
    pub fn get_expr_type(&self) -> expr_node::Type {
        expr_node::Type::ConstantValue
    }
}
impl BoundExpr for BoundLiteral {
    fn return_type(&self) -> DataType {
        self.data_type.clone()
    }
}