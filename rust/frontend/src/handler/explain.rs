use risingwave_sqlparser::ast::Statement;

use crate::pgwire::pg_result::PgResult;

pub(super) fn handle_explain(stmt: Statement, _verbose: bool) -> PgResult {
    // bind, plan, optimize, and serialize here
    format!("{:?}", stmt).into()
}

#[cfg(test)]
mod tests {
    use risingwave_sqlparser::parser::Parser;

    use crate::handler;

    #[test]
    fn handle_explain() {
        let sql = "explain values (11, 22), (33, 44);";
        let stmt = Parser::parse_sql(sql).unwrap().into_iter().next().unwrap();
        let result = handler::handle(stmt);
        let row = result.iter().next().unwrap();
        let s = row[0].as_ref().unwrap().as_utf8();
        assert!(s.contains("11"));
        assert!(s.contains("22"));
        assert!(s.contains("33"));
        assert!(s.contains("44"));
    }
}