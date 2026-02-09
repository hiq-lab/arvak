//! Lexer for `OpenQASM` 3.

use logos::Logos;

/// Tokens for `OpenQASM` 3.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n]+")]
#[logos(skip r"//[^\n]*")]
#[logos(skip r"/\*[^*]*\*+(?:[^/*][^*]*\*+)*/")]
pub enum Token {
    // Keywords
    #[token("OPENQASM")]
    OpenQasm,

    #[token("include")]
    Include,

    #[token("qubit")]
    Qubit,

    #[token("bit")]
    Bit,

    #[token("int")]
    Int,

    #[token("float")]
    Float,

    #[token("bool")]
    Bool,

    #[token("const")]
    Const,

    #[token("let")]
    Let,

    #[token("gate")]
    Gate,

    #[token("def")]
    Def,

    #[token("if")]
    If,

    #[token("else")]
    Else,

    #[token("for")]
    For,

    #[token("while")]
    While,

    #[token("in")]
    In,

    #[token("return")]
    Return,

    #[token("measure")]
    Measure,

    #[token("reset")]
    Reset,

    #[token("barrier")]
    Barrier,

    #[token("delay")]
    Delay,

    #[token("input")]
    Input,

    #[token("output")]
    Output,

    // Built-in gates (higher priority than identifier)
    #[token("U", priority = 3)]
    GateU,

    #[token("CX", priority = 3)]
    GateCX,

    // Constants
    #[token("pi")]
    Pi,

    #[token("tau")]
    Tau,

    #[token("euler")]
    Euler,

    #[token("true")]
    True,

    #[token("false")]
    False,

    // Literals
    #[regex(r"[0-9]+\.[0-9]*([eE][+-]?[0-9]+)?", |lex| lex.slice().parse::<f64>().ok())]
    #[regex(r"[0-9]+[eE][+-]?[0-9]+", |lex| lex.slice().parse::<f64>().ok())]
    FloatLiteral(f64),

    #[regex(r"[0-9]+", |lex| lex.slice().parse::<u64>().ok())]
    IntLiteral(u64),

    #[regex(r#""[^"]*""#, |lex| {
        let s = lex.slice();
        Some(s[1..s.len()-1].to_string())
    })]
    StringLiteral(String),

    // Identifiers
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Identifier(String),

    // Operators and punctuation
    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("*")]
    Star,

    #[token("/")]
    Slash,

    #[token("%")]
    Percent,

    #[token("**")]
    Power,

    #[token("==")]
    EqEq,

    #[token("!=")]
    NotEq,

    #[token("<")]
    Lt,

    #[token("<=")]
    LtEq,

    #[token(">")]
    Gt,

    #[token(">=")]
    GtEq,

    #[token("&&")]
    And,

    #[token("||")]
    Or,

    #[token("!")]
    Not,

    #[token("~")]
    Tilde,

    #[token("&")]
    Ampersand,

    #[token("|")]
    Pipe,

    #[token("^")]
    Caret,

    #[token("<<")]
    LShift,

    #[token(">>")]
    RShift,

    #[token("=")]
    Eq,

    #[token("+=")]
    PlusEq,

    #[token("-=")]
    MinusEq,

    #[token("*=")]
    StarEq,

    #[token("/=")]
    SlashEq,

    #[token("->")]
    Arrow,

    #[token("@")]
    At,

    #[token("(")]
    LParen,

    #[token(")")]
    RParen,

    #[token("[")]
    LBracket,

    #[token("]")]
    RBracket,

    #[token("{")]
    LBrace,

    #[token("}")]
    RBrace,

    #[token(";")]
    Semicolon,

    #[token(":")]
    Colon,

    #[token(",")]
    Comma,

    #[token(".")]
    Dot,
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::OpenQasm => write!(f, "OPENQASM"),
            Token::Include => write!(f, "include"),
            Token::Qubit => write!(f, "qubit"),
            Token::Bit => write!(f, "bit"),
            Token::Int => write!(f, "int"),
            Token::Float => write!(f, "float"),
            Token::Bool => write!(f, "bool"),
            Token::Const => write!(f, "const"),
            Token::Let => write!(f, "let"),
            Token::Gate => write!(f, "gate"),
            Token::Def => write!(f, "def"),
            Token::If => write!(f, "if"),
            Token::Else => write!(f, "else"),
            Token::For => write!(f, "for"),
            Token::While => write!(f, "while"),
            Token::In => write!(f, "in"),
            Token::Return => write!(f, "return"),
            Token::Measure => write!(f, "measure"),
            Token::Reset => write!(f, "reset"),
            Token::Barrier => write!(f, "barrier"),
            Token::Delay => write!(f, "delay"),
            Token::Input => write!(f, "input"),
            Token::Output => write!(f, "output"),
            Token::GateU => write!(f, "U"),
            Token::GateCX => write!(f, "CX"),
            Token::Pi => write!(f, "pi"),
            Token::Tau => write!(f, "tau"),
            Token::Euler => write!(f, "euler"),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::FloatLiteral(v) => write!(f, "{v}"),
            Token::IntLiteral(v) => write!(f, "{v}"),
            Token::StringLiteral(s) => write!(f, "\"{s}\""),
            Token::Identifier(s) => write!(f, "{s}"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Percent => write!(f, "%"),
            Token::Power => write!(f, "**"),
            Token::EqEq => write!(f, "=="),
            Token::NotEq => write!(f, "!="),
            Token::Lt => write!(f, "<"),
            Token::LtEq => write!(f, "<="),
            Token::Gt => write!(f, ">"),
            Token::GtEq => write!(f, ">="),
            Token::And => write!(f, "&&"),
            Token::Or => write!(f, "||"),
            Token::Not => write!(f, "!"),
            Token::Tilde => write!(f, "~"),
            Token::Ampersand => write!(f, "&"),
            Token::Pipe => write!(f, "|"),
            Token::Caret => write!(f, "^"),
            Token::LShift => write!(f, "<<"),
            Token::RShift => write!(f, ">>"),
            Token::Eq => write!(f, "="),
            Token::PlusEq => write!(f, "+="),
            Token::MinusEq => write!(f, "-="),
            Token::StarEq => write!(f, "*="),
            Token::SlashEq => write!(f, "/="),
            Token::Arrow => write!(f, "->"),
            Token::At => write!(f, "@"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::Semicolon => write!(f, ";"),
            Token::Colon => write!(f, ":"),
            Token::Comma => write!(f, ","),
            Token::Dot => write!(f, "."),
        }
    }
}

/// A token with its span information.
#[derive(Debug, Clone)]
pub struct SpannedToken {
    pub token: Token,
    #[allow(dead_code)]
    pub span: std::ops::Range<usize>,
}

/// Tokenize a QASM3 source string.
pub fn tokenize(source: &str) -> Vec<Result<SpannedToken, (std::ops::Range<usize>, String)>> {
    let mut lexer = Token::lexer(source);
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        let span = lexer.span();
        if let Ok(token) = result {
            tokens.push(Ok(SpannedToken { token, span }));
        } else {
            let slice = &source[span.clone()];
            tokens.push(Err((span, format!("Invalid token: '{slice}'"))));
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let source = "OPENQASM 3.0;";
        let tokens: Vec<_> = tokenize(source)
            .into_iter()
            .filter_map(Result::ok)
            .collect();

        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].token, Token::OpenQasm);
        assert!(matches!(tokens[1].token, Token::FloatLiteral(v) if (v - 3.0).abs() < 0.001));
        assert_eq!(tokens[2].token, Token::Semicolon);
    }

    #[test]
    fn test_qubit_declaration() {
        let source = "qubit[2] q;";
        let tokens: Vec<_> = tokenize(source)
            .into_iter()
            .filter_map(Result::ok)
            .collect();

        assert_eq!(tokens[0].token, Token::Qubit);
        assert_eq!(tokens[1].token, Token::LBracket);
        assert!(matches!(tokens[2].token, Token::IntLiteral(2)));
        assert_eq!(tokens[3].token, Token::RBracket);
        assert!(matches!(tokens[4].token, Token::Identifier(ref s) if s == "q"));
        assert_eq!(tokens[5].token, Token::Semicolon);
    }

    #[test]
    fn test_gate_call() {
        let source = "h q[0];";
        let tokens: Vec<_> = tokenize(source)
            .into_iter()
            .filter_map(Result::ok)
            .collect();

        assert!(matches!(tokens[0].token, Token::Identifier(ref s) if s == "h"));
        assert!(matches!(tokens[1].token, Token::Identifier(ref s) if s == "q"));
        assert_eq!(tokens[2].token, Token::LBracket);
        assert!(matches!(tokens[3].token, Token::IntLiteral(0)));
        assert_eq!(tokens[4].token, Token::RBracket);
        assert_eq!(tokens[5].token, Token::Semicolon);
    }

    #[test]
    fn test_parameterized_gate() {
        let source = "rx(pi/2) q[0];";
        let tokens: Vec<_> = tokenize(source)
            .into_iter()
            .filter_map(Result::ok)
            .collect();

        assert!(matches!(tokens[0].token, Token::Identifier(ref s) if s == "rx"));
        assert_eq!(tokens[1].token, Token::LParen);
        assert_eq!(tokens[2].token, Token::Pi);
        assert_eq!(tokens[3].token, Token::Slash);
        assert!(matches!(tokens[4].token, Token::IntLiteral(2)));
        assert_eq!(tokens[5].token, Token::RParen);
    }

    #[test]
    fn test_comments() {
        let source = r"
            // This is a comment
            qubit q;
            /* Multi-line
               comment */
            bit c;
        ";
        let tokens: Vec<_> = tokenize(source)
            .into_iter()
            .filter_map(Result::ok)
            .collect();

        // Should only have: qubit, q, ;, bit, c, ;
        assert_eq!(tokens.len(), 6);
    }
}
