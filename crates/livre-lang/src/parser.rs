//! Recursive-descent parser producing [`Program`] from tokens.

use std::rc::Rc;

use crate::ast::*;
use crate::error::LangError;
use crate::lexer::{tokenize, Kw, Tok, Token};

pub fn parse(src: &str) -> Result<Program, LangError> {
    let toks = tokenize(src)?;
    let mut p = Parser { toks, pos: 0, index_depth: 0 };
    p.program()
}

/// Parses a single expression (property defaults, `str2func`).
pub fn parse_expr(src: &str) -> Result<Expr, LangError> {
    let toks = tokenize(src)?;
    let mut p = Parser { toks, pos: 0, index_depth: 0 };
    let e = p.expr()?;
    p.skip_newlines();
    if !matches!(p.peek(), Tok::Eof) {
        return Err(p.unexpected("end of expression"));
    }
    Ok(e)
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    /// > 0 while parsing index arguments, where `end` and bare `:` are values.
    index_depth: usize,
}

type PResult<T> = Result<T, LangError>;

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn peek_at(&self, k: usize) -> &Tok {
        &self.toks[(self.pos + k).min(self.toks.len() - 1)].tok
    }

    fn line(&self) -> usize {
        self.toks[self.pos].line
    }

    fn bump(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Tok::Op(o) if *o == op)
    }

    fn eat_op(&mut self, op: &str) -> bool {
        if self.is_op(op) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_op(&mut self, op: &str) -> PResult<()> {
        if self.eat_op(op) {
            Ok(())
        } else {
            Err(self.unexpected(&format!("'{op}'")))
        }
    }

    fn unexpected(&self, wanted: &str) -> LangError {
        let found = match self.peek() {
            Tok::Num(n) => format!("number {n}"),
            Tok::Imag(n) => format!("number {n}i"),
            Tok::Chars(s) => format!("'{s}'"),
            Tok::Str(s) => format!("\"{s}\""),
            Tok::Ident(s) => format!("'{s}'"),
            Tok::Kw(k) => format!("keyword '{}'", format!("{k:?}").to_lowercase()),
            Tok::CmdArg(s) => format!("'{s}'"),
            Tok::Op(o) => format!("'{o}'"),
            Tok::Sep(true) => "';'".into(),
            Tok::Sep(false) => "','".into(),
            Tok::Newline => "end of line".into(),
            Tok::Eof => "end of input".into(),
        };
        LangError::new(format!("Invalid expression: expected {wanted} but found {found}."), self.line())
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Tok::Newline | Tok::Sep(_)) {
            self.bump();
        }
    }

    fn program(&mut self) -> PResult<Program> {
        let mut prog = Program::default();
        loop {
            self.skip_newlines();
            match self.peek() {
                Tok::Eof => break,
                Tok::Kw(Kw::Function) => {
                    let f = self.function()?;
                    prog.functions.push(Rc::new(f));
                }
                Tok::Kw(Kw::End) => return Err(LangError::new("Illegal use of reserved keyword \"end\".", self.line())),
                _ => {
                    let s = self.statement()?;
                    prog.body.push(s);
                }
            }
        }
        Ok(prog)
    }

    fn function(&mut self) -> PResult<Function> {
        self.bump(); // function
        let mut outputs = Vec::new();
        // Forms: function name, function name(args), function out = name(..), function [a,b] = name(..)
        if self.eat_op("[") {
            while !self.is_op("]") {
                match self.bump() {
                    Tok::Ident(n) => outputs.push(n),
                    Tok::Sep(_) => {}
                    _ => return Err(self.unexpected("output name")),
                }
            }
            self.bump();
            self.expect_op("=")?;
        } else if matches!(self.peek_at(1), Tok::Op("=")) {
            if let Tok::Ident(n) = self.bump() {
                outputs.push(n);
            }
            self.bump();
        }
        let mut name = match self.bump() {
            Tok::Ident(n) => n,
            _ => return Err(self.unexpected("function name")),
        };
        // Property accessors: `get.Name` / `set.Name`.
        if self.is_op(".") {
            if let Tok::Ident(m) = self.peek_at(1).clone() {
                self.bump();
                self.bump();
                name = format!("{name}.{m}");
            }
        }
        let mut params = Vec::new();
        if self.eat_op("(") {
            while !self.is_op(")") {
                match self.bump() {
                    Tok::Ident(n) => params.push(n),
                    Tok::Op("~") => params.push("~".into()),
                    Tok::Sep(false) => {}
                    _ => return Err(self.unexpected("parameter name")),
                }
            }
            self.bump();
        }
        // Command-syntax lexing may have turned `function foo bar` into args; ignore them.
        while matches!(self.peek(), Tok::CmdArg(_)) {
            self.bump();
        }
        let body = self.block(&[Kw::End, Kw::Function])?;
        if matches!(self.peek(), Tok::Kw(Kw::End)) {
            self.bump();
        }
        Ok(Function { name, params, outputs, body })
    }

    /// Statements until one of `stops` (not consumed) or end of input.
    fn block(&mut self, stops: &[Kw]) -> PResult<Vec<Stmt>> {
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                Tok::Eof => return Ok(out),
                Tok::Kw(k) if stops.contains(k) => return Ok(out),
                _ => out.push(self.statement()?),
            }
        }
    }

    fn expect_end(&mut self, what: &str, line: usize) -> PResult<()> {
        if matches!(self.peek(), Tok::Kw(Kw::End)) {
            self.bump();
            Ok(())
        } else {
            Err(LangError::new(format!("'{what}' at line {line} is missing its matching 'end'."), line))
        }
    }

    /// Consumes the statement terminator; returns whether output is shown.
    fn terminator(&mut self) -> PResult<bool> {
        match self.peek() {
            Tok::Sep(semi) => {
                let show = !*semi;
                self.bump();
                Ok(show)
            }
            Tok::Newline => {
                self.bump();
                Ok(true)
            }
            Tok::Eof | Tok::Kw(_) => Ok(true),
            _ => Err(self.unexpected("end of statement")),
        }
    }

    fn statement(&mut self) -> PResult<Stmt> {
        let line = self.line();
        let kind = match self.peek().clone() {
            Tok::Kw(Kw::If) => {
                self.bump();
                let mut clauses = Vec::new();
                let cond = self.expr()?;
                let body = self.block(&[Kw::Elseif, Kw::Else, Kw::End])?;
                clauses.push((cond, body));
                let mut otherwise = None;
                loop {
                    match self.peek() {
                        Tok::Kw(Kw::Elseif) => {
                            self.bump();
                            let c = self.expr()?;
                            let b = self.block(&[Kw::Elseif, Kw::Else, Kw::End])?;
                            clauses.push((c, b));
                        }
                        Tok::Kw(Kw::Else) => {
                            self.bump();
                            otherwise = Some(self.block(&[Kw::End])?);
                        }
                        _ => break,
                    }
                }
                self.expect_end("if", line)?;
                StmtKind::If { clauses, otherwise }
            }
            Tok::Kw(Kw::For) | Tok::Kw(Kw::Parfor) => {
                self.bump();
                let paren = self.eat_op("(");
                let var = match self.bump() {
                    Tok::Ident(n) => n,
                    _ => return Err(self.unexpected("loop variable")),
                };
                self.expect_op("=")?;
                let iter = self.expr()?;
                if paren {
                    self.expect_op(")")?;
                }
                let body = self.block(&[Kw::End])?;
                self.expect_end("for", line)?;
                StmtKind::For { var, iter, body }
            }
            Tok::Kw(Kw::While) => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block(&[Kw::End])?;
                self.expect_end("while", line)?;
                StmtKind::While { cond, body }
            }
            Tok::Kw(Kw::Switch) => {
                self.bump();
                let subject = self.expr()?;
                self.skip_newlines();
                let mut cases = Vec::new();
                let mut otherwise = None;
                loop {
                    self.skip_newlines();
                    match self.peek() {
                        Tok::Kw(Kw::Case) => {
                            self.bump();
                            let v = self.expr()?;
                            let b = self.block(&[Kw::Case, Kw::Otherwise, Kw::End])?;
                            cases.push((v, b));
                        }
                        Tok::Kw(Kw::Otherwise) => {
                            self.bump();
                            otherwise = Some(self.block(&[Kw::Case, Kw::Otherwise, Kw::End])?);
                        }
                        _ => break,
                    }
                }
                self.expect_end("switch", line)?;
                StmtKind::Switch { subject, cases, otherwise }
            }
            Tok::Kw(Kw::Try) => {
                self.bump();
                let body = self.block(&[Kw::Catch, Kw::End])?;
                let mut catch_var = None;
                let mut catch_body = Vec::new();
                if matches!(self.peek(), Tok::Kw(Kw::Catch)) {
                    let catch_line = self.line();
                    self.bump();
                    if let Tok::Ident(n) = self.peek().clone() {
                        if self.line() == catch_line {
                            self.bump();
                            catch_var = Some(n);
                        }
                    }
                    // `catch err` lexes `err ...` as a command when followed by words; skip args.
                    while matches!(self.peek(), Tok::CmdArg(_)) {
                        self.bump();
                    }
                    catch_body = self.block(&[Kw::End])?;
                }
                self.expect_end("try", line)?;
                StmtKind::Try { body, catch_var, catch_body }
            }
            Tok::Kw(Kw::Global) | Tok::Kw(Kw::Persistent) => {
                self.bump();
                let mut names = Vec::new();
                while let Tok::Ident(n) | Tok::CmdArg(n) = self.peek().clone() {
                    self.bump();
                    names.push(n);
                }
                self.terminator()?;
                StmtKind::Global(names)
            }
            Tok::Kw(Kw::Break) => {
                self.bump();
                self.terminator()?;
                StmtKind::Break
            }
            Tok::Kw(Kw::Continue) => {
                self.bump();
                self.terminator()?;
                StmtKind::Continue
            }
            Tok::Kw(Kw::Return) => {
                self.bump();
                self.terminator()?;
                StmtKind::Return
            }
            Tok::Kw(Kw::Function) => {
                return Err(LangError::new("Function definitions must appear at the end of the script.", line));
            }
            Tok::Kw(k) => {
                return Err(LangError::new(format!("Illegal use of reserved keyword \"{}\".", format!("{k:?}").to_lowercase()), line));
            }
            Tok::Ident(name) if matches!(self.peek_at(1), Tok::CmdArg(_)) => {
                self.bump();
                let mut args = Vec::new();
                while let Tok::CmdArg(a) = self.peek().clone() {
                    self.bump();
                    args.push(a);
                }
                let print = self.terminator()?;
                StmtKind::Command { name, args, print }
            }
            _ => {
                let e = self.expr()?;
                if self.is_op("=") {
                    self.bump();
                    let lhs = match e {
                        Expr::Matrix(rows) if rows.len() <= 1 => {
                            let mut v = Vec::new();
                            for item in rows.into_iter().flatten() {
                                v.push(to_lvalue(item, line)?);
                            }
                            v
                        }
                        other => vec![to_lvalue(other, line)?],
                    };
                    let rhs = self.expr()?;
                    let print = self.terminator()?;
                    StmtKind::Assign { lhs, rhs, print }
                } else {
                    let print = self.terminator()?;
                    StmtKind::Expr { expr: e, print }
                }
            }
        };
        Ok(Stmt { kind, line })
    }

    // ---- expressions -------------------------------------------------------------------

    pub fn expr(&mut self) -> PResult<Expr> {
        self.oror()
    }

    fn oror(&mut self) -> PResult<Expr> {
        let mut l = self.andand()?;
        while self.eat_op("||") {
            let r = self.andand()?;
            l = Expr::Binary(BinOp::OrOr, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn andand(&mut self) -> PResult<Expr> {
        let mut l = self.or()?;
        while self.eat_op("&&") {
            let r = self.or()?;
            l = Expr::Binary(BinOp::AndAnd, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn or(&mut self) -> PResult<Expr> {
        let mut l = self.and()?;
        while self.eat_op("|") {
            let r = self.and()?;
            l = Expr::Binary(BinOp::Or, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn and(&mut self) -> PResult<Expr> {
        let mut l = self.cmp()?;
        while self.eat_op("&") {
            let r = self.cmp()?;
            l = Expr::Binary(BinOp::And, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn cmp(&mut self) -> PResult<Expr> {
        let mut l = self.range()?;
        loop {
            let op = match self.peek() {
                Tok::Op("==") => BinOp::Eq,
                Tok::Op("~=") => BinOp::Ne,
                Tok::Op("<") => BinOp::Lt,
                Tok::Op("<=") => BinOp::Le,
                Tok::Op(">") => BinOp::Gt,
                Tok::Op(">=") => BinOp::Ge,
                _ => return Ok(l),
            };
            self.bump();
            let r = self.range()?;
            l = Expr::Binary(op, Box::new(l), Box::new(r));
        }
    }

    fn range(&mut self) -> PResult<Expr> {
        // A bare `:` index argument.
        if self.index_depth > 0 && self.is_op(":") && matches!(self.peek_at(1), Tok::Op(")") | Tok::Op("}") | Tok::Sep(false)) {
            self.bump();
            return Ok(Expr::Colon);
        }
        let a = self.additive()?;
        if !self.is_op(":") {
            return Ok(a);
        }
        self.bump();
        let b = self.additive()?;
        if self.eat_op(":") {
            let c = self.additive()?;
            Ok(Expr::Range(Box::new(a), Some(Box::new(b)), Box::new(c)))
        } else {
            Ok(Expr::Range(Box::new(a), None, Box::new(b)))
        }
    }

    fn additive(&mut self) -> PResult<Expr> {
        let mut l = self.mult()?;
        loop {
            let op = match self.peek() {
                Tok::Op("+") => BinOp::Add,
                Tok::Op("-") => BinOp::Sub,
                _ => return Ok(l),
            };
            self.bump();
            let r = self.mult()?;
            l = Expr::Binary(op, Box::new(l), Box::new(r));
        }
    }

    fn mult(&mut self) -> PResult<Expr> {
        let mut l = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Op("*") => BinOp::MatMul,
                Tok::Op(".*") => BinOp::Mul,
                Tok::Op("/") => BinOp::MatDiv,
                Tok::Op("./") => BinOp::Div,
                Tok::Op("\\") => BinOp::MatLeftDiv,
                Tok::Op(".\\") => BinOp::LeftDiv,
                _ => return Ok(l),
            };
            self.bump();
            let r = self.unary()?;
            l = Expr::Binary(op, Box::new(l), Box::new(r));
        }
    }

    fn unary(&mut self) -> PResult<Expr> {
        let op = match self.peek() {
            Tok::Op("-") => UnOp::Neg,
            Tok::Op("+") => UnOp::Plus,
            Tok::Op("~") => {
                // `[~, i] = max(x)` placeholder.
                if matches!(self.peek_at(1), Tok::Sep(false) | Tok::Op("]")) {
                    self.bump();
                    return Ok(Expr::Tilde);
                }
                UnOp::Not
            }
            _ => return self.power(),
        };
        self.bump();
        let e = self.unary()?;
        Ok(Expr::Unary(op, Box::new(e)))
    }

    fn power(&mut self) -> PResult<Expr> {
        let mut l = self.postfix()?;
        loop {
            let op = match self.peek() {
                Tok::Op("^") => BinOp::MatPow,
                Tok::Op(".^") => BinOp::Pow,
                _ => return Ok(l),
            };
            self.bump();
            let r = self.power_operand()?;
            l = Expr::Binary(op, Box::new(l), Box::new(r));
        }
    }

    fn power_operand(&mut self) -> PResult<Expr> {
        let op = match self.peek() {
            Tok::Op("-") => UnOp::Neg,
            Tok::Op("+") => UnOp::Plus,
            Tok::Op("~") => UnOp::Not,
            _ => return self.postfix(),
        };
        self.bump();
        Ok(Expr::Unary(op, Box::new(self.power_operand()?)))
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        loop {
            // Inside a matrix the lexer already split `a (1)` into two elements, so a `(`
            // reaching here always indexes.
            if self.is_op("(") {
                self.bump();
                let args = self.args(")")?;
                e = Expr::Index(Box::new(e), args);
            } else if self.is_op("{") {
                self.bump();
                let args = self.args("}")?;
                e = Expr::CellIndex(Box::new(e), args);
            } else if self.is_op(".") {
                self.bump();
                match self.bump() {
                    Tok::Ident(n) => e = Expr::Field(Box::new(e), n),
                    Tok::Kw(k) => e = Expr::Field(Box::new(e), format!("{k:?}").to_lowercase()),
                    Tok::Op("(") => {
                        let saved = self.index_depth;
                        self.index_depth = 0;
                        let f = self.expr()?;
                        self.index_depth = saved;
                        self.expect_op(")")?;
                        e = Expr::DynField(Box::new(e), Box::new(f));
                    }
                    _ => return Err(self.unexpected("field name")),
                }
            } else if self.is_op("'") {
                self.bump();
                e = Expr::Unary(UnOp::CTranspose, Box::new(e));
            } else if self.is_op(".'") {
                self.bump();
                e = Expr::Unary(UnOp::Transpose, Box::new(e));
            } else {
                return Ok(e);
            }
        }
    }

    fn args(&mut self, close: &str) -> PResult<Vec<Expr>> {
        self.index_depth += 1;
        let mut out = Vec::new();
        if self.eat_op(close) {
            self.index_depth -= 1;
            return Ok(out);
        }
        loop {
            out.push(self.expr()?);
            if self.eat_op(close) {
                break;
            }
            match self.bump() {
                Tok::Sep(false) => {}
                _ => {
                    self.index_depth -= 1;
                    self.pos -= 1;
                    return Err(self.unexpected(&format!("',' or '{close}'")));
                }
            }
        }
        self.index_depth -= 1;
        Ok(out)
    }

    fn primary(&mut self) -> PResult<Expr> {
        match self.bump() {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Imag(n) => Ok(Expr::Imag(n)),
            Tok::Chars(s) => Ok(Expr::Chars(s)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Ident(n) => {
                // `obj@Super(...)` / `method@Super(...)`.
                if self.is_op("@") && !self.toks[self.pos].space_before {
                    if let Tok::Ident(class) = self.peek_at(1).clone() {
                        self.bump();
                        self.bump();
                        let mut class = class;
                        while self.is_op(".") {
                            if let Tok::Ident(m) = self.peek_at(1).clone() {
                                self.bump();
                                self.bump();
                                class = format!("{class}.{m}");
                            } else {
                                break;
                            }
                        }
                        return Ok(Expr::SuperRef { name: n, class });
                    }
                }
                Ok(Expr::Ident(n))
            }
            Tok::Kw(Kw::End) if self.index_depth > 0 => Ok(Expr::End),
            Tok::Op("(") => {
                let saved = self.index_depth;
                self.index_depth = 0;
                let e = self.expr()?;
                self.index_depth = saved;
                self.expect_op(")")?;
                // Parenthesized expressions are values: `(a)(1)` is not supported, but
                // wrapping keeps `(x)'` correct.
                Ok(e)
            }
            Tok::Op("[") => Ok(Expr::Matrix(self.matrix_rows("]")?)),
            Tok::Op("{") => Ok(Expr::Cell(self.matrix_rows("}")?)),
            Tok::Op("@") => {
                if self.eat_op("(") {
                    let mut params = Vec::new();
                    while !self.is_op(")") {
                        match self.bump() {
                            Tok::Ident(n) => params.push(n),
                            Tok::Op("~") => params.push("~".into()),
                            Tok::Sep(false) => {}
                            _ => return Err(self.unexpected("parameter name")),
                        }
                    }
                    self.bump();
                    let saved = self.index_depth;
                    self.index_depth = 0;
                    let body = self.expr()?;
                    self.index_depth = saved;
                    let text = format!("@({}){}", params.join(","), unparse(&body));
                    Ok(Expr::AnonFn(Rc::new(AnonFn { params, body, text })))
                } else {
                    match self.bump() {
                        Tok::Ident(n) => {
                            let mut name = n;
                            // Package-qualified names such as @pkg.fn.
                            while self.is_op(".") {
                                if let Tok::Ident(m) = self.peek_at(1).clone() {
                                    self.bump();
                                    self.bump();
                                    name = format!("{name}.{m}");
                                } else {
                                    break;
                                }
                            }
                            Ok(Expr::FuncHandle(name))
                        }
                        _ => Err(self.unexpected("function name")),
                    }
                }
            }
            _ => {
                self.pos -= 1;
                Err(self.unexpected("an expression"))
            }
        }
    }

    fn matrix_rows(&mut self, close: &str) -> PResult<Vec<Vec<Expr>>> {
        let saved = self.index_depth;
        // `end` keeps working inside a matrix within an index: a([1 end]).
        let mut rows: Vec<Vec<Expr>> = Vec::new();
        let mut row: Vec<Expr> = Vec::new();
        loop {
            match self.peek() {
                Tok::Op(o) if *o == close => {
                    self.bump();
                    break;
                }
                Tok::Sep(true) | Tok::Newline => {
                    self.bump();
                    if !row.is_empty() {
                        rows.push(std::mem::take(&mut row));
                    }
                }
                Tok::Sep(false) => {
                    self.bump();
                }
                Tok::Eof => {
                    self.index_depth = saved;
                    return Err(self.unexpected(&format!("'{close}'")));
                }
                _ => {
                    let e = self.expr()?;
                    row.push(e);
                }
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }
        self.index_depth = saved;
        Ok(rows)
    }
}

fn to_lvalue(e: Expr, line: usize) -> PResult<LValue> {
    fn chain(e: Expr, acc: &mut Vec<Accessor>, line: usize) -> PResult<String> {
        match e {
            Expr::Ident(n) => Ok(n),
            Expr::Index(b, args) => {
                let n = chain(*b, acc, line)?;
                acc.push(Accessor::Paren(args));
                Ok(n)
            }
            Expr::CellIndex(b, args) => {
                let n = chain(*b, acc, line)?;
                acc.push(Accessor::Brace(args));
                Ok(n)
            }
            Expr::Field(b, f) => {
                let n = chain(*b, acc, line)?;
                acc.push(Accessor::Field(f));
                Ok(n)
            }
            Expr::DynField(b, f) => {
                let n = chain(*b, acc, line)?;
                acc.push(Accessor::DynField(*f));
                Ok(n)
            }
            _ => Err(LangError::new("Invalid target for assignment.", line)),
        }
    }
    if matches!(e, Expr::Tilde) {
        return Ok(LValue::Tilde);
    }
    let mut acc = Vec::new();
    let name = chain(e, &mut acc, line)?;
    Ok(LValue::Target { name, chain: acc })
}

/// Source-like rendering of an expression (used to display anonymous functions).
pub fn unparse(e: &Expr) -> String {
    fn list(v: &[Expr]) -> String {
        v.iter().map(unparse).collect::<Vec<_>>().join(",")
    }
    fn rows(r: &[Vec<Expr>]) -> String {
        r.iter().map(|row| list(row)).collect::<Vec<_>>().join(";")
    }
    match e {
        Expr::Num(n) => crate::display::fmt_g(*n, 15),
        Expr::Imag(n) => format!("{}i", crate::display::fmt_g(*n, 15)),
        Expr::SuperRef { name, class } => format!("{name}@{class}"),
        Expr::Chars(s) => format!("'{}'", s.replace('\'', "''")),
        Expr::Str(s) => format!("\"{}\"", s.replace('"', "\"\"")),
        Expr::Ident(n) => n.clone(),
        Expr::Colon => ":".into(),
        Expr::End => "end".into(),
        Expr::Tilde => "~".into(),
        Expr::Unary(op, x) => match op {
            UnOp::Neg => format!("-{}", unparse(x)),
            UnOp::Plus => format!("+{}", unparse(x)),
            UnOp::Not => format!("~{}", unparse(x)),
            UnOp::CTranspose => format!("{}'", unparse(x)),
            UnOp::Transpose => format!("{}.'", unparse(x)),
        },
        Expr::Binary(op, a, b) => {
            let wrap = |x: &Expr| match x {
                Expr::Binary(..) | Expr::Range(..) => format!("({})", unparse(x)),
                _ => unparse(x),
            };
            format!("{}{}{}", wrap(a), op.symbol(), wrap(b))
        }
        Expr::Range(a, s, b) => match s {
            Some(s) => format!("{}:{}:{}", unparse(a), unparse(s), unparse(b)),
            None => format!("{}:{}", unparse(a), unparse(b)),
        },
        Expr::Index(b, a) => format!("{}({})", unparse(b), list(a)),
        Expr::CellIndex(b, a) => format!("{}{{{}}}", unparse(b), list(a)),
        Expr::Field(b, f) => format!("{}.{}", unparse(b), f),
        Expr::DynField(b, f) => format!("{}.({})", unparse(b), unparse(f)),
        Expr::Matrix(r) => format!("[{}]", rows(r)),
        Expr::Cell(r) => format!("{{{}}}", rows(r)),
        Expr::AnonFn(f) => f.text.clone(),
        Expr::FuncHandle(n) => format!("@{n}"),
    }
}
