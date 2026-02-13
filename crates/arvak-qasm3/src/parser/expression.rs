//! Expression parsing for QASM3.

use super::Parser;
use crate::ast::{BinOp, Expression};
use crate::error::{ParseError, ParseResult};
use crate::lexer::Token;

impl Parser {
    /// Parse an expression.
    pub(super) fn parse_expression(&mut self) -> ParseResult<Expression> {
        self.parse_binary_expr(0)
    }

    /// Parse binary expression with precedence climbing.
    fn parse_binary_expr(&mut self, min_prec: u8) -> ParseResult<Expression> {
        let mut left = self.parse_unary_expr()?;

        while let Some(op) = self.peek_binary_op() {
            let prec = op_precedence(op);
            if prec < min_prec {
                break;
            }
            self.advance(); // consume operator

            let right = self.parse_binary_expr(prec + 1)?;
            left = Expression::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse unary expression.
    fn parse_unary_expr(&mut self) -> ParseResult<Expression> {
        if self.consume(&Token::Minus) {
            let expr = self.parse_unary_expr()?;
            return Ok(Expression::Neg(Box::new(expr)));
        }
        if self.consume(&Token::Not) {
            // TODO: Logical NOT (!) has different semantics than arithmetic negation (-).
            // This is a known simplification.
            let expr = self.parse_unary_expr()?;
            return Ok(Expression::Neg(Box::new(expr)));
        }
        self.parse_primary_expr()
    }

    /// Parse primary expression.
    fn parse_primary_expr(&mut self) -> ParseResult<Expression> {
        let token = self
            .peek()
            .cloned()
            .ok_or_else(|| ParseError::UnexpectedEof("expression".into()))?;

        match token {
            Token::IntLiteral(v) => {
                self.advance();
                // Note: u64 to i64 cast may wrap for values > i64::MAX.
                // Very large integer literals are uncommon in QASM3.
                Ok(Expression::Int(v as i64))
            }
            Token::FloatLiteral(v) => {
                self.advance();
                Ok(Expression::Float(v))
            }
            Token::Pi => {
                self.advance();
                Ok(Expression::Pi)
            }
            Token::Tau => {
                self.advance();
                Ok(Expression::Tau)
            }
            Token::Euler => {
                self.advance();
                Ok(Expression::Euler)
            }
            Token::True => {
                self.advance();
                Ok(Expression::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(Expression::Bool(false))
            }
            Token::Identifier(name) => {
                self.advance();
                // Check for function call
                if self.consume(&Token::LParen) {
                    let args = self.parse_expression_list()?;
                    self.expect(Token::RParen)?;
                    Ok(Expression::FnCall { name, args })
                } else {
                    Ok(Expression::Identifier(name))
                }
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(Token::RParen)?;
                Ok(Expression::Paren(Box::new(expr)))
            }
            _ => Err(ParseError::UnexpectedToken {
                line: self.line,
                expected: "expression".into(),
                found: token.to_string(),
            }),
        }
    }

    /// Peek at binary operator.
    fn peek_binary_op(&self) -> Option<BinOp> {
        match self.peek()? {
            Token::Plus => Some(BinOp::Add),
            Token::Minus => Some(BinOp::Sub),
            Token::Star => Some(BinOp::Mul),
            Token::Slash => Some(BinOp::Div),
            Token::Percent => Some(BinOp::Mod),
            Token::Power => Some(BinOp::Pow),
            Token::EqEq => Some(BinOp::Eq),
            Token::NotEq => Some(BinOp::NotEq),
            Token::Lt => Some(BinOp::Lt),
            Token::LtEq => Some(BinOp::LtEq),
            Token::Gt => Some(BinOp::Gt),
            Token::GtEq => Some(BinOp::GtEq),
            Token::And => Some(BinOp::And),
            Token::Or => Some(BinOp::Or),
            Token::Ampersand => Some(BinOp::BitAnd),
            Token::Pipe => Some(BinOp::BitOr),
            Token::Caret => Some(BinOp::BitXor),
            _ => None,
        }
    }

    /// Parse expression list.
    pub(super) fn parse_expression_list(&mut self) -> ParseResult<Vec<Expression>> {
        if self.check(&Token::RParen) {
            return Ok(vec![]);
        }
        let mut exprs = vec![self.parse_expression()?];
        while self.consume(&Token::Comma) {
            exprs.push(self.parse_expression()?);
        }
        Ok(exprs)
    }
}

/// Get operator precedence.
fn op_precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 1,
        BinOp::And => 2,
        BinOp::BitOr => 3,
        BinOp::BitXor => 4,
        BinOp::BitAnd => 5,
        BinOp::Eq | BinOp::NotEq => 6,
        BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => 7,
        BinOp::LShift | BinOp::RShift => 8,
        BinOp::Add | BinOp::Sub => 9,
        BinOp::Mul | BinOp::Div | BinOp::Mod => 10,
        BinOp::Pow => 11,
    }
}
