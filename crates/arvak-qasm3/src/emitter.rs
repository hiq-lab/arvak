//! QASM3 emitter for serializing circuits.

use arvak_ir::{
    Circuit, GateKind, Instruction, InstructionKind, ParameterExpression, StandardGate,
};

use crate::error::ParseResult;

/// Emit a circuit as QASM3 source code.
pub fn emit(circuit: &Circuit) -> ParseResult<String> {
    let mut emitter = Emitter::new();
    emitter.emit_circuit(circuit)
}

/// QASM3 emitter.
struct Emitter {
    output: String,
    // TODO: Use indent field for nested structure formatting
    indent: usize,
}

#[allow(clippy::unused_self, clippy::unnecessary_wraps)]
impl Emitter {
    fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
        }
    }

    fn emit_circuit(&mut self, circuit: &Circuit) -> ParseResult<String> {
        // Version
        self.writeln("OPENQASM 3.0;");
        self.writeln("");

        // Qubit declarations
        let num_qubits = circuit.num_qubits();
        if num_qubits > 0 {
            self.writeln(&format!("qubit[{num_qubits}] q;"));
        }

        // Classical bit declarations
        let num_clbits = circuit.num_clbits();
        if num_clbits > 0 {
            self.writeln(&format!("bit[{num_clbits}] c;"));
        }

        if num_qubits > 0 || num_clbits > 0 {
            self.writeln("");
        }

        // Instructions
        for (_, instruction) in circuit.dag().topological_ops() {
            self.emit_instruction(instruction)?;
        }

        Ok(self.output.clone())
    }

    fn emit_instruction(&mut self, instruction: &Instruction) -> ParseResult<()> {
        match &instruction.kind {
            InstructionKind::Gate(gate) => {
                let name = self.emit_gate_name(&gate.kind);
                let params = self.emit_gate_params(&gate.kind);
                let qubits = self.emit_qubits(&instruction.qubits);

                if params.is_empty() {
                    self.writeln(&format!("{name} {qubits};"));
                } else {
                    self.writeln(&format!("{name}({params}) {qubits};"));
                }
            }

            InstructionKind::Measure => {
                let qubits = self.emit_qubits(&instruction.qubits);
                let clbits = self.emit_clbits(&instruction.clbits);

                if instruction.qubits.len() == 1 {
                    self.writeln(&format!("{clbits} = measure {qubits};"));
                } else {
                    // Broadcast measurement
                    for (q, c) in instruction.qubits.iter().zip(instruction.clbits.iter()) {
                        self.writeln(&format!("c[{}] = measure q[{}];", c.0, q.0));
                    }
                }
            }

            InstructionKind::Reset => {
                let qubits = self.emit_qubits(&instruction.qubits);
                self.writeln(&format!("reset {qubits};"));
            }

            InstructionKind::Barrier => {
                let qubits = self.emit_qubits(&instruction.qubits);
                if qubits.is_empty() {
                    self.writeln("barrier;");
                } else {
                    self.writeln(&format!("barrier {qubits};"));
                }
            }

            InstructionKind::Delay { duration } => {
                let qubits = self.emit_qubits(&instruction.qubits);
                self.writeln(&format!("delay[{duration}] {qubits};"));
            }

            InstructionKind::Shuttle { from_zone, to_zone } => {
                // Shuttle is a neutral-atom specific instruction; emit as pragma
                let qubits = self.emit_qubits(&instruction.qubits);
                self.writeln(&format!(
                    "// @pragma shuttle({from_zone}, {to_zone}) {qubits};"
                ));
            }

            InstructionKind::NoiseChannel { model, role } => {
                // Noise channels have no QASM3 equivalent; emit as pragma comment
                let qubits = self.emit_qubits(&instruction.qubits);
                self.writeln(&format!("// @pragma noise_{role}({model}) {qubits};"));
            }
        }

        Ok(())
    }

    fn emit_gate_name(&self, kind: &GateKind) -> String {
        match kind {
            GateKind::Standard(std) => match std {
                StandardGate::I => "id".into(),
                StandardGate::X => "x".into(),
                StandardGate::Y => "y".into(),
                StandardGate::Z => "z".into(),
                StandardGate::H => "h".into(),
                StandardGate::S => "s".into(),
                StandardGate::Sdg => "sdg".into(),
                StandardGate::T => "t".into(),
                StandardGate::Tdg => "tdg".into(),
                StandardGate::SX => "sx".into(),
                StandardGate::SXdg => "sxdg".into(),
                StandardGate::Rx(_) => "rx".into(),
                StandardGate::Ry(_) => "ry".into(),
                StandardGate::Rz(_) => "rz".into(),
                StandardGate::P(_) => "p".into(),
                StandardGate::U(_, _, _) => "u".into(),
                StandardGate::CX => "cx".into(),
                StandardGate::CY => "cy".into(),
                StandardGate::CZ => "cz".into(),
                StandardGate::CH => "ch".into(),
                StandardGate::Swap => "swap".into(),
                StandardGate::ISwap => "iswap".into(),
                StandardGate::CRx(_) => "crx".into(),
                StandardGate::CRy(_) => "cry".into(),
                StandardGate::CRz(_) => "crz".into(),
                StandardGate::CP(_) => "cp".into(),
                StandardGate::RXX(_) => "rxx".into(),
                StandardGate::RYY(_) => "ryy".into(),
                StandardGate::RZZ(_) => "rzz".into(),
                StandardGate::CCX => "ccx".into(),
                StandardGate::CSwap => "cswap".into(),
                StandardGate::PRX(_, _) => "prx".into(),
            },
            GateKind::Custom(custom) => custom.name.clone(),
        }
    }

    fn emit_gate_params(&self, kind: &GateKind) -> String {
        match kind {
            GateKind::Standard(std) => {
                let params = std.parameters();
                if params.is_empty() {
                    String::new()
                } else {
                    params
                        .iter()
                        .map(|p| self.emit_param(p))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            }
            GateKind::Custom(custom) => custom
                .params
                .iter()
                .map(|p| self.emit_param(p))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    #[allow(clippy::only_used_in_recursion, clippy::self_only_used_in_recursion)]
    fn emit_param(&self, param: &ParameterExpression) -> String {
        match param {
            ParameterExpression::Constant(v) => {
                // Check if close to common fractions of pi
                let pi = std::f64::consts::PI;
                if (*v - pi).abs() < 1e-10 {
                    "pi".into()
                } else if (*v - pi / 2.0).abs() < 1e-10 {
                    "pi/2".into()
                } else if (*v - pi / 4.0).abs() < 1e-10 {
                    "pi/4".into()
                } else if (*v + pi / 2.0).abs() < 1e-10 {
                    "-pi/2".into()
                } else if (*v + pi / 4.0).abs() < 1e-10 {
                    "-pi/4".into()
                } else {
                    format!("{v:.6}")
                }
            }
            ParameterExpression::Symbol(name) => name.clone(),
            ParameterExpression::Pi => "pi".into(),
            ParameterExpression::Neg(e) => format!("-({})", self.emit_param(e)),
            ParameterExpression::Add(a, b) => {
                format!("({} + {})", self.emit_param(a), self.emit_param(b))
            }
            ParameterExpression::Sub(a, b) => {
                format!("({} - {})", self.emit_param(a), self.emit_param(b))
            }
            ParameterExpression::Mul(a, b) => {
                format!("({} * {})", self.emit_param(a), self.emit_param(b))
            }
            ParameterExpression::Div(a, b) => {
                format!("({} / {})", self.emit_param(a), self.emit_param(b))
            }
        }
    }

    fn emit_qubits(&self, qubits: &[arvak_ir::QubitId]) -> String {
        qubits
            .iter()
            .map(|q| format!("q[{}]", q.0))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn emit_clbits(&self, clbits: &[arvak_ir::ClbitId]) -> String {
        if clbits.len() == 1 {
            format!("c[{}]", clbits[0].0)
        } else {
            clbits
                .iter()
                .map(|c| format!("c[{}]", c.0))
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn writeln(&mut self, line: &str) {
        let indent = "    ".repeat(self.indent);
        self.output.push_str(&indent);
        self.output.push_str(line);
        self.output.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::QubitId;

    #[test]
    fn test_emit_bell_state() {
        let circuit = Circuit::bell().unwrap();
        let qasm = emit(&circuit).unwrap();

        assert!(qasm.contains("OPENQASM 3.0;"));
        assert!(qasm.contains("qubit[2] q;"));
        assert!(qasm.contains("bit[2] c;"));
        assert!(qasm.contains("h q[0];"));
        assert!(qasm.contains("cx q[0], q[1];"));
    }

    #[test]
    fn test_emit_parameterized() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.rx(std::f64::consts::PI / 2.0, QubitId(0)).unwrap();

        let qasm = emit(&circuit).unwrap();
        assert!(qasm.contains("rx(pi/2) q[0];"));
    }

    #[test]
    fn test_roundtrip() {
        let original = r"OPENQASM 3.0;
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
";

        let circuit = crate::parse(original).unwrap();
        let emitted = emit(&circuit).unwrap();

        // Parse again
        let circuit2 = crate::parse(&emitted).unwrap();
        assert_eq!(circuit.num_qubits(), circuit2.num_qubits());
        assert_eq!(circuit.depth(), circuit2.depth());
    }

    #[test]
    fn test_roundtrip_missing_gates() {
        // Test all 7 gates that were previously missing from the parser
        let source = r"OPENQASM 3.0;
qubit[2] q;
sxdg q[0];
ch q[0], q[1];
crx(pi/4) q[0], q[1];
cry(pi/4) q[0], q[1];
rxx(pi/4) q[0], q[1];
ryy(pi/4) q[0], q[1];
rzz(pi/4) q[0], q[1];
";

        let circuit = crate::parse(source).unwrap();
        let emitted = emit(&circuit).unwrap();

        // Verify emitted output contains all gates
        assert!(emitted.contains("sxdg q[0];"));
        assert!(emitted.contains("ch q[0], q[1];"));
        assert!(emitted.contains("crx("));
        assert!(emitted.contains("cry("));
        assert!(emitted.contains("rxx("));
        assert!(emitted.contains("ryy("));
        assert!(emitted.contains("rzz("));

        // Parse again to verify full roundtrip
        let circuit2 = crate::parse(&emitted).unwrap();
        assert_eq!(circuit.num_qubits(), circuit2.num_qubits());
        assert_eq!(circuit.depth(), circuit2.depth());
    }
}
