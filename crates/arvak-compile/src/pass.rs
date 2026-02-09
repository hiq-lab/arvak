//! Pass trait and types for compilation passes.

use arvak_ir::CircuitDag;

use crate::error::CompileResult;
use crate::property::PropertySet;

/// The kind of compilation pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassKind {
    /// Analysis pass that reads but does not modify the DAG.
    Analysis,
    /// Transformation pass that modifies the DAG.
    Transformation,
}

/// A compilation pass that operates on a circuit DAG.
///
/// Passes are the fundamental unit of compilation in Arvak. Each pass
/// performs a specific transformation or analysis on the circuit.
pub trait Pass: Send + Sync {
    /// Get the name of this pass.
    fn name(&self) -> &str;

    /// Get the kind of this pass.
    fn kind(&self) -> PassKind;

    /// Run the pass on the given DAG.
    ///
    /// For analysis passes, this should not modify the DAG but may
    /// write to the `PropertySet`.
    ///
    /// For transformation passes, this modifies the DAG and may read
    /// from the `PropertySet`.
    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()>;

    /// Check if this pass should run based on current state.
    ///
    /// This can be overridden to skip passes that are not needed.
    fn should_run(&self, _dag: &CircuitDag, _properties: &PropertySet) -> bool {
        true
    }
}

/// Marker trait for analysis passes.
///
/// Analysis passes read the DAG and write to the `PropertySet`.
/// They should NOT modify the DAG.
pub trait AnalysisPass: Send + Sync {
    /// Get the name of this analysis pass.
    fn name(&self) -> &str;

    /// Analyze the circuit and update properties.
    fn analyze(&self, dag: &CircuitDag, properties: &mut PropertySet) -> CompileResult<()>;

    /// Check if this pass should run.
    fn should_run(&self, _dag: &CircuitDag, _properties: &PropertySet) -> bool {
        true
    }
}

/// Marker trait for transformation passes.
///
/// Transformation passes modify the DAG.
/// They may read from the `PropertySet` but should NOT modify it.
pub trait TransformationPass: Send + Sync {
    /// Get the name of this transformation pass.
    fn name(&self) -> &str;

    /// Transform the circuit.
    fn transform(&self, dag: &mut CircuitDag, properties: &PropertySet) -> CompileResult<()>;

    /// Check if this pass should run.
    fn should_run(&self, _dag: &CircuitDag, _properties: &PropertySet) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPass;

    impl Pass for TestPass {
        fn name(&self) -> &'static str {
            "test"
        }

        fn kind(&self) -> PassKind {
            PassKind::Transformation
        }

        fn run(&self, _dag: &mut CircuitDag, _properties: &mut PropertySet) -> CompileResult<()> {
            Ok(())
        }
    }

    #[test]
    fn test_pass_kind() {
        let pass = TestPass;
        assert_eq!(pass.kind(), PassKind::Transformation);
        assert_eq!(pass.name(), "test");
    }
}
