/// Column ID is the unique identifier of a column in a table. Different from table ID,
/// column ID is not globally unique.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColumnId(i32);

impl ColumnId {
    pub fn get_id(&self) -> i32 {
        self.0
    }
}

impl From<i32> for ColumnId {
    fn from(column_id: i32) -> Self {
        ColumnId(column_id)
    }
}

impl std::fmt::Display for ColumnId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}