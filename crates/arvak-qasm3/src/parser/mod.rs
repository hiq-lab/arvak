//! Parser for `OpenQASM` 3.

mod expression;
mod lowering;
mod statement;

pub(crate) use lowering::lower_to_circuit;

use arvak_ir::Circuit;

use crate::ast::Program;
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

/// Maximum expression nesting depth. Recursive-descent parsing recurses once
/// per nesting level; without a limit, adversarial input such as thousands of
/// nested parentheses overflows the stack and aborts the process.
pub(super) const MAX_EXPR_DEPTH: usize = 256;

/// Parser state.
pub(super) struct Parser {
    pub(super) tokens: Vec<SpannedToken>,
    pub(super) pos: usize,
    /// Byte offset of every `\n` in the source, for span → line lookup.
    newline_offsets: Vec<usize>,
    /// Current expression recursion depth (guarded by `MAX_EXPR_DEPTH`).
    pub(super) expr_depth: usize,
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

        let newline_offsets = source
            .bytes()
            .enumerate()
            .filter_map(|(i, b)| (b == b'\n').then_some(i))
            .collect();

        Ok(Self {
            tokens,
            pos: 0,
            newline_offsets,
            expr_depth: 0,
        })
    }

    /// 1-based line number of a byte offset in the source.
    fn line_of_offset(&self, offset: usize) -> usize {
        self.newline_offsets.partition_point(|&nl| nl < offset) + 1
    }

    /// Line of the most recently consumed token — use for errors raised
    /// after `advance()`/`expect()`.
    pub(super) fn line(&self) -> usize {
        let last = self.tokens.len().saturating_sub(1);
        self.tokens
            .get(self.pos.saturating_sub(1).min(last))
            .map_or(1, |t| self.line_of_offset(t.span.start))
    }

    /// Line of the next (peeked) token — use for errors raised before
    /// consuming it.
    pub(super) fn peek_line(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map_or_else(|| self.line(), |t| self.line_of_offset(t.span.start))
    }

    /// Check if we've reached the end.
    pub(super) fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Peek at the current token.
    pub(super) fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.token)
    }

    /// Advance and return the current token.
    pub(super) fn advance(&mut self) -> Option<Token> {
        if self.is_eof() {
            return None;
        }
        let token = self.tokens[self.pos].token.clone();
        self.pos += 1;
        Some(token)
    }

    /// Expect a specific token.
    #[allow(clippy::needless_pass_by_value)]
    pub(super) fn expect(&mut self, expected: Token) -> ParseResult<()> {
        let found = self
            .advance()
            .ok_or_else(|| ParseError::UnexpectedEof(format!("expected {expected}")))?;

        if std::mem::discriminant(&found) != std::mem::discriminant(&expected) {
            return Err(ParseError::UnexpectedToken {
                line: self.line(),
                expected: expected.to_string(),
                found: found.to_string(),
            });
        }
        Ok(())
    }

    /// Check if current token matches.
    pub(super) fn check(&self, token: &Token) -> bool {
        self.peek()
            .is_some_and(|t| std::mem::discriminant(t) == std::mem::discriminant(token))
    }

    /// Consume token if it matches.
    pub(super) fn consume(&mut self, token: &Token) -> bool {
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

    /// Parse identifier list.
    pub(super) fn parse_identifier_list(&mut self) -> ParseResult<Vec<String>> {
        let mut ids = vec![self.parse_identifier()?];
        while self.consume(&Token::Comma) {
            ids.push(self.parse_identifier()?);
        }
        Ok(ids)
    }

    /// Parse an identifier.
    pub(super) fn parse_identifier(&mut self) -> ParseResult<String> {
        match self.advance() {
            Some(Token::Identifier(s)) => Ok(s),
            Some(other) => Err(ParseError::UnexpectedToken {
                line: self.line(),
                expected: "identifier".into(),
                found: other.to_string(),
            }),
            None => Err(ParseError::UnexpectedEof("identifier".into())),
        }
    }

    /// Parse an integer literal.
    pub(super) fn parse_int_literal(&mut self) -> ParseResult<u64> {
        match self.advance() {
            Some(Token::IntLiteral(v)) => Ok(v),
            Some(other) => Err(ParseError::UnexpectedToken {
                line: self.line(),
                expected: "integer".into(),
                found: other.to_string(),
            }),
            None => Err(ParseError::UnexpectedEof("integer".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse errors must report the line of the offending token, not
    /// always "line 1" (IQM reviewer feedback: the constant line number
    /// made errors in longer programs impossible to locate).
    #[test]
    fn test_error_reports_correct_line_for_unexpected_statement() {
        // `)` on line 4 cannot start a statement.
        let source = "OPENQASM 3.0;\nqubit[2] q;\nh q[0];\n) q[0];\n";
        let err = parse(source).unwrap_err().to_string();
        assert!(err.contains("line 4"), "expected line 4 in: {err}");
    }

    #[test]
    fn test_error_reports_correct_line_for_failed_expect() {
        // Missing `]` on line 3.
        let source = "OPENQASM 3.0;\nqubit[2] q;\nbit[2 c;\nh q[0];\n";
        let err = parse(source).unwrap_err().to_string();
        assert!(err.contains("line 3"), "expected line 3 in: {err}");
    }

    #[test]
    fn test_error_reports_correct_line_for_bad_expression() {
        // `;` where an expression is required, on line 5.
        let source = "OPENQASM 3.0;\nqubit[1] q;\nh q[0];\nx q[0];\nrz(;) q[0];\n";
        let err = parse(source).unwrap_err().to_string();
        assert!(err.contains("line 5"), "expected line 5 in: {err}");
    }

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

    #[test]
    fn test_deeply_nested_expression_rejected() {
        // Adversarial input: thousands of nested parentheses previously
        // overflowed the stack (recursive descent without a depth limit).
        let depth = 100_000;
        let expr = format!("{}pi{}", "(".repeat(depth), ")".repeat(depth));
        let source = format!("OPENQASM 3.0;\nqubit q;\nrx({expr}) q;\n");

        let result = parse(&source);
        assert!(matches!(result, Err(ParseError::ExpressionTooDeep(_))));
    }

    #[test]
    fn test_reasonable_nesting_accepted() {
        // Ordinary nesting depths must keep working.
        let source = "OPENQASM 3.0;\nqubit q;\nrx((((pi / 2) + 0.1) * 2)) q;\n";
        parse(source).unwrap();
    }
}
