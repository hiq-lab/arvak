//! AST-to-Circuit lowering for QASM3.

use arvak_ir::{Circuit, ClbitId, ParameterExpression, QubitId};
use rustc_hash::FxHashMap;

use crate::ast::{BinOp, BitRef, Expression, GateCall, Program, QubitRef, Statement};
use crate::error::{ParseError, ParseResult};

/// Lower an AST Program to a Circuit.
pub(crate) fn lower_to_circuit(program: &Program) -> ParseResult<Circuit> {
    let mut lowerer = Lowerer::new();
    lowerer.lower(program)
}

/// Lowers AST to Circuit.
struct Lowerer {
    /// Qubit registers: name -> (`start_id`, size).
    qregs: FxHashMap<String, (u32, u32)>,
    /// Classical bit registers: name -> (`start_id`, size).
    cregs: FxHashMap<String, (u32, u32)>,
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
            qregs: FxHashMap::default(),
            cregs: FxHashMap::default(),
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

                // If bits is empty, create matching bits.
                // Assumption: qubit IDs map directly to classical bit IDs (i.e.,
                // qubit N is measured into clbit N). This only holds when qubit
                // and clbit registers are declared with matching sizes and order.
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
                // Classical assignment skipped (not yet supported).
                // TODO: Implement classical variable assignment lowering.
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
                // Identity gates are intentionally dropped during lowering
                // since they have no effect on circuit state.
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
