//! Parser for `OpenQASM` 3.

use std::collections::HashMap;

use arvak_ir::{Circuit, ClbitId, ParameterExpression, QubitId};

use crate::ast::{BinOp, BitRef, Expression, GateCall, Program, QubitRef, Range, Statement};
use crate::error::{ParseError, ParseResult};
use crate::lexer::{SpannedToken, Token, tokenize};

/// Parse a QASM3 source string into a Circuit.
pub fn parse(source: &str) -> ParseResult<Circuit> {
    let mut parser = Parser::new(source)?;
    let program = parser.parse_program()?;
    lower_to_circuit(&program)
}

/// Parse a QASM3 source string into an AST Program.
#[allow(dead_code)]
pub fn parse_ast(source: &str) -> ParseResult<Program> {
    let mut parser = Parser::new(source)?;
    parser.parse_program()
}

/// Parser state.
struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
    line: usize,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    clippy::only_used_in_recursion
)]
impl Parser {
    /// Create a new parser from source.
    fn new(source: &str) -> ParseResult<Self> {
        let token_results = tokenize(source);
        let mut tokens = Vec::new();

        for result in token_results {
            match result {
                Ok(t) => tokens.push(t),
                Err((span, msg)) => {
                    return Err(ParseError::LexerError {
                        position: span.start,
                        message: msg,
                    });
                }
            }
        }

        Ok(Self {
            tokens,
            pos: 0,
            line: 1,
        })
    }

    /// Check if we've reached the end.
    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Peek at the current token.
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.token)
    }

    /// Advance and return the current token.
    fn advance(&mut self) -> Option<Token> {
        if self.is_eof() {
            return None;
        }
        let token = self.tokens[self.pos].token.clone();
        self.pos += 1;
        Some(token)
    }

    /// Expect a specific token.
    #[allow(clippy::needless_pass_by_value)]
    fn expect(&mut self, expected: Token) -> ParseResult<()> {
        let found = self
            .advance()
            .ok_or_else(|| ParseError::UnexpectedEof(format!("expected {expected}")))?;

        if std::mem::discriminant(&found) != std::mem::discriminant(&expected) {
            return Err(ParseError::UnexpectedToken {
                line: self.line,
                expected: expected.to_string(),
                found: found.to_string(),
            });
        }
        Ok(())
    }

    /// Check if current token matches.
    fn check(&self, token: &Token) -> bool {
        self.peek()
            .is_some_and(|t| std::mem::discriminant(t) == std::mem::discriminant(token))
    }

    /// Consume token if it matches.
    fn consume(&mut self, token: &Token) -> bool {
        if self.check(token) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Parse the entire program.
    fn parse_program(&mut self) -> ParseResult<Program> {
        // Parse version
        self.expect(Token::OpenQasm)?;
        let version = self.parse_version()?;
        self.expect(Token::Semicolon)?;

        // Parse statements
        let mut statements = Vec::new();
        while !self.is_eof() {
            statements.push(self.parse_statement()?);
        }

        Ok(Program {
            version,
            statements,
        })
    }

    /// Parse version number.
    fn parse_version(&mut self) -> ParseResult<String> {
        match self.advance() {
            Some(Token::FloatLiteral(v)) => Ok(format!("{v}")),
            Some(Token::IntLiteral(v)) => Ok(format!("{v}.0")),
            Some(other) => Err(ParseError::InvalidVersion(other.to_string())),
            None => Err(ParseError::UnexpectedEof("version number".into())),
        }
    }

    /// Parse a statement.
    fn parse_statement(&mut self) -> ParseResult<Statement> {
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
            Some(size as u32)
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
            Some(size as u32)
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
    fn parse_block_or_statement(&mut self) -> ParseResult<Vec<Statement>> {
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

    /// Parse an expression.
    fn parse_expression(&mut self) -> ParseResult<Expression> {
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
            let expr = self.parse_unary_expr()?;
            return Ok(Expression::Neg(Box::new(expr))); // Simplified
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
    fn parse_expression_list(&mut self) -> ParseResult<Vec<Expression>> {
        if self.check(&Token::RParen) {
            return Ok(vec![]);
        }
        let mut exprs = vec![self.parse_expression()?];
        while self.consume(&Token::Comma) {
            exprs.push(self.parse_expression()?);
        }
        Ok(exprs)
    }

    /// Parse identifier list.
    fn parse_identifier_list(&mut self) -> ParseResult<Vec<String>> {
        let mut ids = vec![self.parse_identifier()?];
        while self.consume(&Token::Comma) {
            ids.push(self.parse_identifier()?);
        }
        Ok(ids)
    }

    /// Parse an identifier.
    fn parse_identifier(&mut self) -> ParseResult<String> {
        match self.advance() {
            Some(Token::Identifier(s)) => Ok(s),
            Some(other) => Err(ParseError::UnexpectedToken {
                line: self.line,
                expected: "identifier".into(),
                found: other.to_string(),
            }),
            None => Err(ParseError::UnexpectedEof("identifier".into())),
        }
    }

    /// Parse an integer literal.
    fn parse_int_literal(&mut self) -> ParseResult<u64> {
        match self.advance() {
            Some(Token::IntLiteral(v)) => Ok(v),
            Some(other) => Err(ParseError::UnexpectedToken {
                line: self.line,
                expected: "integer".into(),
                found: other.to_string(),
            }),
            None => Err(ParseError::UnexpectedEof("integer".into())),
        }
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

/// Lower an AST Program to a Circuit.
fn lower_to_circuit(program: &Program) -> ParseResult<Circuit> {
    let mut lowerer = Lowerer::new();
    lowerer.lower(program)
}

/// Lowers AST to Circuit.
struct Lowerer {
    /// Qubit registers: name -> (`start_id`, size).
    qregs: HashMap<String, (u32, u32)>,
    /// Classical bit registers: name -> (`start_id`, size).
    cregs: HashMap<String, (u32, u32)>,
    /// Next qubit ID.
    next_qubit: u32,
    /// Next clbit ID.
    next_clbit: u32,
}

#[allow(
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::unused_self,
    clippy::unnecessary_wraps
)]
impl Lowerer {
    fn new() -> Self {
        Self {
            qregs: HashMap::new(),
            cregs: HashMap::new(),
            next_qubit: 0,
            next_clbit: 0,
        }
    }

    fn lower(&mut self, program: &Program) -> ParseResult<Circuit> {
        // First pass: collect declarations
        for stmt in &program.statements {
            match stmt {
                Statement::QubitDecl { name, size } => {
                    let size = size.unwrap_or(1);
                    self.qregs.insert(name.clone(), (self.next_qubit, size));
                    self.next_qubit += size;
                }
                Statement::BitDecl { name, size } => {
                    let size = size.unwrap_or(1);
                    self.cregs.insert(name.clone(), (self.next_clbit, size));
                    self.next_clbit += size;
                }
                _ => {}
            }
        }

        // Create circuit
        let mut circuit = Circuit::with_size("qasm_circuit", self.next_qubit, self.next_clbit);

        // Second pass: lower statements
        for stmt in &program.statements {
            self.lower_statement(&mut circuit, stmt)?;
        }

        Ok(circuit)
    }

    fn lower_statement(&self, circuit: &mut Circuit, stmt: &Statement) -> ParseResult<()> {
        match stmt {
            Statement::QubitDecl { .. } | Statement::BitDecl { .. } | Statement::Include(_) => {
                // Already handled
                Ok(())
            }

            Statement::Gate(call) => self.lower_gate_call(circuit, call),

            Statement::Measure { qubits, bits } => {
                let q_ids = self.resolve_qubits(qubits)?;
                let c_ids = self.resolve_clbits(bits)?;

                // If bits is empty, create matching bits
                let c_ids = if c_ids.is_empty() {
                    q_ids.iter().map(|q| ClbitId(q.0)).collect()
                } else {
                    c_ids
                };

                for (q, c) in q_ids.iter().zip(c_ids.iter()) {
                    circuit.measure(*q, *c)?;
                }
                Ok(())
            }

            Statement::Reset { qubits } => {
                let q_ids = self.resolve_qubits(qubits)?;
                for q in q_ids {
                    circuit.reset(q)?;
                }
                Ok(())
            }

            Statement::Barrier { qubits } => {
                let q_ids = self.resolve_qubits(qubits)?;
                if q_ids.is_empty() {
                    circuit.barrier_all()?;
                } else {
                    circuit.barrier(q_ids)?;
                }
                Ok(())
            }

            Statement::If { .. } => {
                // TODO: Implement conditional execution
                Err(ParseError::Generic(
                    "If statements not yet supported".into(),
                ))
            }

            Statement::For { .. } => {
                // TODO: Implement loops
                Err(ParseError::Generic("For loops not yet supported".into()))
            }

            Statement::GateDef { .. } => {
                // TODO: Implement custom gate definitions
                Err(ParseError::Generic(
                    "Custom gate definitions not yet supported".into(),
                ))
            }

            Statement::Assignment { .. } => {
                // Classical assignments - skip for now
                Ok(())
            }

            Statement::Delay { .. } => {
                // TODO: Implement delays
                Ok(())
            }
        }
    }

    fn lower_gate_call(&self, circuit: &mut Circuit, call: &GateCall) -> ParseResult<()> {
        let qubits = self.resolve_qubits(&call.qubits)?;
        let params: Vec<_> = call
            .params
            .iter()
            .map(expr_to_param)
            .collect::<ParseResult<_>>()?;

        match call.name.to_lowercase().as_str() {
            // Single-qubit gates
            "id" | "i" => {
                // Identity - no-op
                Ok(())
            }
            "x" => {
                for q in qubits {
                    circuit.x(q)?;
                }
                Ok(())
            }
            "y" => {
                for q in qubits {
                    circuit.y(q)?;
                }
                Ok(())
            }
            "z" => {
                for q in qubits {
                    circuit.z(q)?;
                }
                Ok(())
            }
            "h" => {
                for q in qubits {
                    circuit.h(q)?;
                }
                Ok(())
            }
            "s" => {
                for q in qubits {
                    circuit.s(q)?;
                }
                Ok(())
            }
            "sdg" => {
                for q in qubits {
                    circuit.sdg(q)?;
                }
                Ok(())
            }
            "t" => {
                for q in qubits {
                    circuit.t(q)?;
                }
                Ok(())
            }
            "tdg" => {
                for q in qubits {
                    circuit.tdg(q)?;
                }
                Ok(())
            }
            "sx" => {
                for q in qubits {
                    circuit.sx(q)?;
                }
                Ok(())
            }
            "sxdg" => {
                for q in qubits {
                    circuit.sxdg(q)?;
                }
                Ok(())
            }
            "rx" => {
                check_param_count("rx", &params, 1)?;
                for q in qubits {
                    circuit.rx(params[0].clone(), q)?;
                }
                Ok(())
            }
            "ry" => {
                check_param_count("ry", &params, 1)?;
                for q in qubits {
                    circuit.ry(params[0].clone(), q)?;
                }
                Ok(())
            }
            "rz" => {
                check_param_count("rz", &params, 1)?;
                for q in qubits {
                    circuit.rz(params[0].clone(), q)?;
                }
                Ok(())
            }
            "p" | "phase" => {
                check_param_count("p", &params, 1)?;
                for q in qubits {
                    circuit.p(params[0].clone(), q)?;
                }
                Ok(())
            }
            "u" | "u3" => {
                check_param_count("u", &params, 3)?;
                for q in qubits {
                    circuit.u(params[0].clone(), params[1].clone(), params[2].clone(), q)?;
                }
                Ok(())
            }
            "prx" => {
                check_param_count("prx", &params, 2)?;
                for q in qubits {
                    circuit.prx(params[0].clone(), params[1].clone(), q)?;
                }
                Ok(())
            }

            // Two-qubit gates
            "cx" | "cnot" => {
                check_qubit_count("cx", &qubits, 2)?;
                circuit.cx(qubits[0], qubits[1])?;
                Ok(())
            }
            "cy" => {
                check_qubit_count("cy", &qubits, 2)?;
                circuit.cy(qubits[0], qubits[1])?;
                Ok(())
            }
            "cz" => {
                check_qubit_count("cz", &qubits, 2)?;
                circuit.cz(qubits[0], qubits[1])?;
                Ok(())
            }
            "swap" => {
                check_qubit_count("swap", &qubits, 2)?;
                circuit.swap(qubits[0], qubits[1])?;
                Ok(())
            }
            "iswap" => {
                check_qubit_count("iswap", &qubits, 2)?;
                circuit.iswap(qubits[0], qubits[1])?;
                Ok(())
            }
            "crz" => {
                check_param_count("crz", &params, 1)?;
                check_qubit_count("crz", &qubits, 2)?;
                circuit.crz(params[0].clone(), qubits[0], qubits[1])?;
                Ok(())
            }
            "cp" | "cphase" => {
                check_param_count("cp", &params, 1)?;
                check_qubit_count("cp", &qubits, 2)?;
                circuit.cp(params[0].clone(), qubits[0], qubits[1])?;
                Ok(())
            }
            "ch" => {
                check_qubit_count("ch", &qubits, 2)?;
                circuit.ch(qubits[0], qubits[1])?;
                Ok(())
            }
            "crx" => {
                check_param_count("crx", &params, 1)?;
                check_qubit_count("crx", &qubits, 2)?;
                circuit.crx(params[0].clone(), qubits[0], qubits[1])?;
                Ok(())
            }
            "cry" => {
                check_param_count("cry", &params, 1)?;
                check_qubit_count("cry", &qubits, 2)?;
                circuit.cry(params[0].clone(), qubits[0], qubits[1])?;
                Ok(())
            }
            "rxx" => {
                check_param_count("rxx", &params, 1)?;
                check_qubit_count("rxx", &qubits, 2)?;
                circuit.rxx(params[0].clone(), qubits[0], qubits[1])?;
                Ok(())
            }
            "ryy" => {
                check_param_count("ryy", &params, 1)?;
                check_qubit_count("ryy", &qubits, 2)?;
                circuit.ryy(params[0].clone(), qubits[0], qubits[1])?;
                Ok(())
            }
            "rzz" => {
                check_param_count("rzz", &params, 1)?;
                check_qubit_count("rzz", &qubits, 2)?;
                circuit.rzz(params[0].clone(), qubits[0], qubits[1])?;
                Ok(())
            }

            // Three-qubit gates
            "ccx" | "toffoli" => {
                check_qubit_count("ccx", &qubits, 3)?;
                circuit.ccx(qubits[0], qubits[1], qubits[2])?;
                Ok(())
            }
            "cswap" | "fredkin" => {
                check_qubit_count("cswap", &qubits, 3)?;
                circuit.cswap(qubits[0], qubits[1], qubits[2])?;
                Ok(())
            }

            other => Err(ParseError::UnknownGate(other.to_string())),
        }
    }

    fn resolve_qubits(&self, refs: &[QubitRef]) -> ParseResult<Vec<QubitId>> {
        let mut ids = Vec::new();
        for r in refs {
            match r {
                QubitRef::Single { register, index } => {
                    let (start, size) = self
                        .qregs
                        .get(register)
                        .ok_or_else(|| ParseError::UndefinedIdentifier(register.clone()))?;

                    if let Some(idx) = index {
                        if *idx >= *size {
                            return Err(ParseError::IndexOutOfBounds {
                                register: register.clone(),
                                index: *idx as usize,
                                size: *size as usize,
                            });
                        }
                        ids.push(QubitId(start + idx));
                    } else {
                        // Entire register
                        for i in 0..*size {
                            ids.push(QubitId(start + i));
                        }
                    }
                }
                QubitRef::Range {
                    register,
                    start: s,
                    end: e,
                } => {
                    let (base, size) = self
                        .qregs
                        .get(register)
                        .ok_or_else(|| ParseError::UndefinedIdentifier(register.clone()))?;

                    if *e > *size {
                        return Err(ParseError::IndexOutOfBounds {
                            register: register.clone(),
                            index: *e as usize,
                            size: *size as usize,
                        });
                    }

                    for i in *s..*e {
                        ids.push(QubitId(base + i));
                    }
                }
            }
        }
        Ok(ids)
    }

    fn resolve_clbits(&self, refs: &[BitRef]) -> ParseResult<Vec<ClbitId>> {
        let mut ids = Vec::new();
        for r in refs {
            match r {
                BitRef::Single { register, index } => {
                    let (start, size) = self
                        .cregs
                        .get(register)
                        .ok_or_else(|| ParseError::UndefinedIdentifier(register.clone()))?;

                    if let Some(idx) = index {
                        if *idx >= *size {
                            return Err(ParseError::IndexOutOfBounds {
                                register: register.clone(),
                                index: *idx as usize,
                                size: *size as usize,
                            });
                        }
                        ids.push(ClbitId(start + idx));
                    } else {
                        for i in 0..*size {
                            ids.push(ClbitId(start + i));
                        }
                    }
                }
                BitRef::Range {
                    register,
                    start: s,
                    end: e,
                } => {
                    let (base, size) = self
                        .cregs
                        .get(register)
                        .ok_or_else(|| ParseError::UndefinedIdentifier(register.clone()))?;

                    if *e > *size {
                        return Err(ParseError::IndexOutOfBounds {
                            register: register.clone(),
                            index: *e as usize,
                            size: *size as usize,
                        });
                    }

                    for i in *s..*e {
                        ids.push(ClbitId(base + i));
                    }
                }
            }
        }
        Ok(ids)
    }
}

/// Convert AST expression to `ParameterExpression`.
#[allow(clippy::cast_precision_loss)]
fn expr_to_param(expr: &Expression) -> ParseResult<ParameterExpression> {
    Ok(match expr {
        Expression::Int(v) => ParameterExpression::Constant(*v as f64),
        Expression::Float(v) => ParameterExpression::Constant(*v),
        Expression::Pi => ParameterExpression::Pi,
        Expression::Tau => ParameterExpression::Constant(std::f64::consts::TAU),
        Expression::Euler => ParameterExpression::Constant(std::f64::consts::E),
        Expression::Identifier(name) => ParameterExpression::Symbol(name.clone()),
        Expression::Neg(e) => ParameterExpression::Neg(Box::new(expr_to_param(e)?)),
        Expression::BinOp { left, op, right } => {
            let l = Box::new(expr_to_param(left)?);
            let r = Box::new(expr_to_param(right)?);
            match op {
                BinOp::Add => ParameterExpression::Add(l, r),
                BinOp::Sub => ParameterExpression::Sub(l, r),
                BinOp::Mul => ParameterExpression::Mul(l, r),
                BinOp::Div => ParameterExpression::Div(l, r),
                _ => {
                    return Err(ParseError::Generic(format!(
                        "Unsupported operator in parameter: {op:?}"
                    )));
                }
            }
        }
        Expression::Paren(e) => expr_to_param(e)?,
        Expression::FnCall { name, args: _ } => {
            // Handle common math functions
            match name.as_str() {
                "sin" | "cos" | "tan" | "exp" | "ln" | "sqrt" => {
                    // For now, try to evaluate if constant
                    if let Some(v) = expr.as_f64() {
                        ParameterExpression::Constant(v)
                    } else {
                        return Err(ParseError::Generic(format!(
                            "Cannot evaluate function {name} with symbolic arguments"
                        )));
                    }
                }
                _ => {
                    return Err(ParseError::Generic(format!("Unknown function: {name}")));
                }
            }
        }
        _ => {
            return Err(ParseError::Generic(format!(
                "Cannot convert expression to parameter: {expr:?}"
            )));
        }
    })
}

fn check_param_count(
    gate: &str,
    params: &[ParameterExpression],
    expected: usize,
) -> ParseResult<()> {
    if params.len() == expected {
        Ok(())
    } else {
        Err(ParseError::WrongParameterCount {
            gate: gate.into(),
            expected,
            got: params.len(),
        })
    }
}

fn check_qubit_count(gate: &str, qubits: &[QubitId], expected: usize) -> ParseResult<()> {
    if qubits.len() == expected {
        Ok(())
    } else {
        Err(ParseError::WrongQubitCount {
            gate: gate.into(),
            expected,
            got: qubits.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bell_state() {
        let source = r"
            OPENQASM 3.0;
            qubit[2] q;
            bit[2] c;
            h q[0];
            cx q[0], q[1];
            c = measure q;
        ";

        let circuit = parse(source).unwrap();
        assert_eq!(circuit.num_qubits(), 2);
        assert_eq!(circuit.num_clbits(), 2);
    }

    #[test]
    fn test_parse_ghz() {
        let source = r"
            OPENQASM 3.0;
            qubit[3] q;
            bit[3] c;
            h q[0];
            cx q[0], q[1];
            cx q[1], q[2];
            c = measure q;
        ";

        let circuit = parse(source).unwrap();
        assert_eq!(circuit.num_qubits(), 3);
    }

    #[test]
    fn test_parse_parameterized() {
        let source = r"
            OPENQASM 3.0;
            qubit q;
            rx(pi/2) q;
            ry(pi/4) q;
            rz(0.5) q;
        ";

        let circuit = parse(source).unwrap();
        assert_eq!(circuit.num_qubits(), 1);
        assert_eq!(circuit.depth(), 3);
    }

    #[test]
    fn test_parse_multiple_registers() {
        let source = r"
            OPENQASM 3.0;
            qubit[2] q1;
            qubit[2] q2;
            bit[4] c;
            h q1[0];
            cx q1[0], q2[0];
        ";

        let circuit = parse(source).unwrap();
        assert_eq!(circuit.num_qubits(), 4);
    }

    #[test]
    fn test_parse_error_undefined() {
        let source = r"
            OPENQASM 3.0;
            h undefined[0];
        ";

        let result = parse(source);
        assert!(result.is_err());
    }
}
