//! Statement parsing for QASM3.

use super::Parser;
use crate::ast::{BitRef, GateCall, QubitRef, Range, Statement};
use crate::error::{ParseError, ParseResult};
use crate::lexer::Token;

impl Parser {
    /// Parse a statement.
    pub(super) fn parse_statement(&mut self) -> ParseResult<Statement> {
        let token = self
            .peek()
            .cloned()
            .ok_or_else(|| ParseError::UnexpectedEof("statement".into()))?;

        match token {
            Token::Include => self.parse_include(),
            Token::Qubit => self.parse_qubit_decl(),
            Token::Bit => self.parse_bit_decl(),
            Token::Measure => self.parse_measure(),
            Token::Reset => self.parse_reset(),
            Token::Barrier => self.parse_barrier(),
            Token::If => self.parse_if(),
            Token::For => self.parse_for(),
            Token::Gate => self.parse_gate_def(),
            Token::Identifier(_) => self.parse_identifier_statement(),
            _ => Err(ParseError::UnexpectedToken {
                line: self.line,
                expected: "statement".into(),
                found: token.to_string(),
            }),
        }
    }

    /// Parse include statement.
    fn parse_include(&mut self) -> ParseResult<Statement> {
        self.expect(Token::Include)?;
        let path = match self.advance() {
            Some(Token::StringLiteral(s)) => s,
            Some(other) => {
                return Err(ParseError::UnexpectedToken {
                    line: self.line,
                    expected: "string literal".into(),
                    found: other.to_string(),
                });
            }
            None => return Err(ParseError::UnexpectedEof("include path".into())),
        };
        self.expect(Token::Semicolon)?;
        Ok(Statement::Include(path))
    }

    /// Parse qubit declaration.
    fn parse_qubit_decl(&mut self) -> ParseResult<Statement> {
        self.expect(Token::Qubit)?;

        let size = if self.consume(&Token::LBracket) {
            let size = self.parse_int_literal()?;
            self.expect(Token::RBracket)?;
            Some(u32::try_from(size).expect("qubit size exceeds u32::MAX"))
        } else {
            None
        };

        let name = self.parse_identifier()?;
        self.expect(Token::Semicolon)?;

        Ok(Statement::QubitDecl { name, size })
    }

    /// Parse bit declaration.
    fn parse_bit_decl(&mut self) -> ParseResult<Statement> {
        self.expect(Token::Bit)?;

        let size = if self.consume(&Token::LBracket) {
            let size = self.parse_int_literal()?;
            self.expect(Token::RBracket)?;
            Some(u32::try_from(size).expect("bit size exceeds u32::MAX"))
        } else {
            None
        };

        let name = self.parse_identifier()?;
        self.expect(Token::Semicolon)?;

        Ok(Statement::BitDecl { name, size })
    }

    /// Parse measure statement.
    fn parse_measure(&mut self) -> ParseResult<Statement> {
        self.expect(Token::Measure)?;

        let qubits = self.parse_qubit_refs()?;

        // Check for arrow syntax: measure q -> c;
        let bits = if self.consume(&Token::Arrow) {
            self.parse_bit_refs()?
        } else {
            // No explicit target, bits determined by context
            vec![]
        };

        self.expect(Token::Semicolon)?;

        Ok(Statement::Measure { qubits, bits })
    }

    /// Parse reset statement.
    fn parse_reset(&mut self) -> ParseResult<Statement> {
        self.expect(Token::Reset)?;
        let qubits = self.parse_qubit_refs()?;
        self.expect(Token::Semicolon)?;
        Ok(Statement::Reset { qubits })
    }

    /// Parse barrier statement.
    fn parse_barrier(&mut self) -> ParseResult<Statement> {
        self.expect(Token::Barrier)?;
        let qubits = if self.check(&Token::Semicolon) {
            vec![]
        } else {
            self.parse_qubit_refs()?
        };
        self.expect(Token::Semicolon)?;
        Ok(Statement::Barrier { qubits })
    }

    /// Parse if statement.
    fn parse_if(&mut self) -> ParseResult<Statement> {
        self.expect(Token::If)?;
        self.expect(Token::LParen)?;
        let condition = self.parse_expression()?;
        self.expect(Token::RParen)?;

        let then_body = self.parse_block_or_statement()?;

        let else_body = if self.consume(&Token::Else) {
            Some(self.parse_block_or_statement()?)
        } else {
            None
        };

        Ok(Statement::If {
            condition,
            then_body,
            else_body,
        })
    }

    /// Parse for loop.
    fn parse_for(&mut self) -> ParseResult<Statement> {
        self.expect(Token::For)?;
        let variable = self.parse_identifier()?;
        self.expect(Token::In)?;
        self.expect(Token::LBracket)?;
        let start = self.parse_expression()?;
        self.expect(Token::Colon)?;
        let end = self.parse_expression()?;
        let step = if self.consume(&Token::Colon) {
            Some(self.parse_expression()?)
        } else {
            None
        };
        self.expect(Token::RBracket)?;

        let body = self.parse_block_or_statement()?;

        Ok(Statement::For {
            variable,
            range: Range { start, end, step },
            body,
        })
    }

    /// Parse gate definition.
    fn parse_gate_def(&mut self) -> ParseResult<Statement> {
        self.expect(Token::Gate)?;
        let name = self.parse_identifier()?;

        // Parse parameters
        let params = if self.consume(&Token::LParen) {
            let p = self.parse_identifier_list()?;
            self.expect(Token::RParen)?;
            p
        } else {
            vec![]
        };

        // Parse qubits
        let qubits = self.parse_identifier_list()?;

        // Parse body
        self.expect(Token::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&Token::RBrace) {
            body.push(self.parse_statement()?);
        }
        self.expect(Token::RBrace)?;

        Ok(Statement::GateDef {
            name,
            params,
            qubits,
            body,
        })
    }

    /// Parse statement starting with identifier (gate call or assignment).
    fn parse_identifier_statement(&mut self) -> ParseResult<Statement> {
        let name = self.parse_identifier()?;

        // Check for assignment: c = measure q; or c[0] = expr;
        if self.check(&Token::Eq) || self.check(&Token::LBracket) {
            return self.parse_assignment(name);
        }

        // Otherwise it's a gate call
        self.parse_gate_call(name)
    }

    /// Parse assignment statement.
    fn parse_assignment(&mut self, target: String) -> ParseResult<Statement> {
        let index = if self.consume(&Token::LBracket) {
            let idx = self.parse_int_literal()? as u32;
            self.expect(Token::RBracket)?;
            Some(idx)
        } else {
            None
        };

        self.expect(Token::Eq)?;

        // Check for `c = measure q;`
        if self.consume(&Token::Measure) {
            let qubits = self.parse_qubit_refs()?;
            self.expect(Token::Semicolon)?;

            let bits = if let Some(idx) = index {
                vec![BitRef::single(&target, idx)]
            } else {
                vec![BitRef::register(&target)]
            };

            return Ok(Statement::Measure { qubits, bits });
        }

        let value = self.parse_expression()?;
        self.expect(Token::Semicolon)?;

        Ok(Statement::Assignment {
            target,
            index,
            value,
        })
    }

    /// Parse gate call.
    fn parse_gate_call(&mut self, name: String) -> ParseResult<Statement> {
        // Parse parameters
        let params = if self.consume(&Token::LParen) {
            let p = self.parse_expression_list()?;
            self.expect(Token::RParen)?;
            p
        } else {
            vec![]
        };

        // Parse qubits
        let qubits = self.parse_qubit_refs()?;
        self.expect(Token::Semicolon)?;

        Ok(Statement::Gate(GateCall {
            name,
            params,
            qubits,
            modifiers: vec![],
        }))
    }

    /// Parse a block or single statement.
    pub(super) fn parse_block_or_statement(&mut self) -> ParseResult<Vec<Statement>> {
        if self.consume(&Token::LBrace) {
            let mut stmts = Vec::new();
            while !self.check(&Token::RBrace) {
                stmts.push(self.parse_statement()?);
            }
            self.expect(Token::RBrace)?;
            Ok(stmts)
        } else {
            Ok(vec![self.parse_statement()?])
        }
    }

    /// Parse qubit references.
    fn parse_qubit_refs(&mut self) -> ParseResult<Vec<QubitRef>> {
        let mut refs = vec![self.parse_qubit_ref()?];
        while self.consume(&Token::Comma) {
            refs.push(self.parse_qubit_ref()?);
        }
        Ok(refs)
    }

    /// Parse a single qubit reference.
    fn parse_qubit_ref(&mut self) -> ParseResult<QubitRef> {
        let register = self.parse_identifier()?;

        if self.consume(&Token::LBracket) {
            let index = self.parse_int_literal()? as u32;
            self.expect(Token::RBracket)?;
            Ok(QubitRef::Single {
                register,
                index: Some(index),
            })
        } else {
            Ok(QubitRef::Single {
                register,
                index: None,
            })
        }
    }

    /// Parse bit references.
    fn parse_bit_refs(&mut self) -> ParseResult<Vec<BitRef>> {
        let mut refs = vec![self.parse_bit_ref()?];
        while self.consume(&Token::Comma) {
            refs.push(self.parse_bit_ref()?);
        }
        Ok(refs)
    }

    /// Parse a single bit reference.
    fn parse_bit_ref(&mut self) -> ParseResult<BitRef> {
        let register = self.parse_identifier()?;

        if self.consume(&Token::LBracket) {
            let index = self.parse_int_literal()? as u32;
            self.expect(Token::RBracket)?;
            Ok(BitRef::Single {
                register,
                index: Some(index),
            })
        } else {
            Ok(BitRef::Single {
                register,
                index: None,
            })
        }
    }
}
