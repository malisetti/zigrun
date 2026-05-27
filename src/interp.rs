use crate::ast::{BinOp, Expr, Program, Stmt};

pub fn run(program: &Program) -> Result<u8, String> {
    match &program.main.body {
        Stmt::Return(expr) => eval_expr(expr),
    }
}

fn eval_expr(expr: &Expr) -> Result<u8, String> {
    match expr {
        Expr::Int(n) => Ok(*n),
        Expr::BinOp { op, left, right } => {
            let l = eval_expr(left)?;
            let r = eval_expr(right)?;
            match op {
                BinOp::Add => l.checked_add(r).ok_or_else(|| "integer overflow".to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinOp, Expr};
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    #[test]
    fn evaluates_add() {
        let src = "pub fn main() u8 { return 3 + 4; }";
        let tokens = Lexer::new(src).tokenize().unwrap();
        let program = Parser::new(tokens).parse_program().unwrap();
        assert_eq!(run(&program).unwrap(), 7);
    }

    #[test]
    fn unit_eval_binop() {
        let expr = Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(Expr::Int(3)),
            right: Box::new(Expr::Int(4)),
        };
        assert_eq!(eval_expr(&expr).unwrap(), 7);
    }
}
