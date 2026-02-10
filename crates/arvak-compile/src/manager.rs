//! Pass manager for orchestrating compilation.

use tracing::{debug, info, instrument};

use arvak_ir::CircuitDag;

use crate::error::CompileResult;
use crate::pass::Pass;
use crate::passes::{
    BasicRouting, BasisTranslation, MeasurementBarrierVerification, Optimize1qGates, TrivialLayout,
};
use crate::property::{BasisGates, CouplingMap, PropertySet};

/// Manages and executes a sequence of compilation passes.
pub struct PassManager {
    /// The passes to execute, in order.
    passes: Vec<Box<dyn Pass>>,
}

impl PassManager {
    /// Create a new empty pass manager.
    pub fn new() -> Self {
        Self { passes: vec![] }
    }

    /// Add a pass to the manager.
    pub fn add_pass(&mut self, pass: impl Pass + 'static) {
        self.passes.push(Box::new(pass));
    }

    /// Run all passes on the given DAG.
    #[instrument(skip(self, dag, properties))]
    pub fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        info!(
            "Running pass manager with {} passes on circuit with {} qubits",
            self.passes.len(),
            dag.num_qubits()
        );

        for pass in &self.passes {
            if pass.should_run(dag, properties) {
                debug!("Running pass: {}", pass.name());
                pass.run(dag, properties)?;
                // Avoid calling dag.depth() here — it performs a full topological
                // sort (O(V+E)) on every pass and is only used for debug logging.
                debug!("Pass {} completed, ops: {}", pass.name(), dag.num_ops());
            } else {
                debug!("Skipping pass: {}", pass.name());
            }
        }

        info!(
            "Pass manager completed, final depth: {}, ops: {}",
            dag.depth(),
            dag.num_ops()
        );

        Ok(())
    }

    /// Get the number of passes.
    pub fn len(&self) -> usize {
        self.passes.len()
    }

    /// Check if the manager has no passes.
    pub fn is_empty(&self) -> bool {
        self.passes.is_empty()
    }
}

impl Default for PassManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating pass managers with preset configurations.
pub struct PassManagerBuilder {
    /// Optimization level (0-3).
    optimization_level: u8,
    /// Target properties.
    properties: PropertySet,
}

impl PassManagerBuilder {
    /// Create a new builder with default settings.
    pub fn new() -> Self {
        Self {
            optimization_level: 1,
            properties: PropertySet::new(),
        }
    }

    /// Set the optimization level.
    ///
    /// - Level 0: No optimization, only required transformations
    /// - Level 1: Light optimization (default)
    /// - Level 2: Moderate optimization
    /// - Level 3: Heavy optimization
    #[must_use]
    pub fn with_optimization_level(mut self, level: u8) -> Self {
        self.optimization_level = level.min(3);
        self
    }

    /// Set the target properties.
    #[must_use]
    pub fn with_properties(mut self, properties: PropertySet) -> Self {
        self.properties = properties;
        self
    }

    /// Set the target coupling map and basis gates.
    #[must_use]
    pub fn with_target(mut self, coupling_map: CouplingMap, basis_gates: BasisGates) -> Self {
        self.properties.coupling_map = Some(coupling_map);
        self.properties.basis_gates = Some(basis_gates);
        self
    }

    /// Build the pass manager and return it with the properties.
    pub fn build(self) -> (PassManager, PropertySet) {
        let mut pm = PassManager::new();

        // Always add layout pass if we have a coupling map
        if self.properties.coupling_map.is_some() {
            pm.add_pass(TrivialLayout);
        }

        // Add routing if we have a coupling map
        if self.properties.coupling_map.is_some() {
            pm.add_pass(BasicRouting);
        }

        // Add basis translation if we have basis gates
        if self.properties.basis_gates.is_some() {
            pm.add_pass(BasisTranslation);
        }

        // Add optimization passes based on level
        if self.optimization_level >= 1 {
            pm.add_pass(Optimize1qGates::new());
        }

        // Always add measurement barrier verification as the final pass
        // to catch any correctness violations from optimization passes.
        if self.optimization_level >= 1 {
            pm.add_pass(MeasurementBarrierVerification);
        }

        (pm, self.properties)
    }
}

impl Default for PassManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::{Circuit, QubitId};

    #[test]
    fn test_empty_pass_manager() {
        let pm = PassManager::new();
        assert!(pm.is_empty());
        assert_eq!(pm.len(), 0);
    }

    #[test]
    fn test_pass_manager_run() {
        let pm = PassManager::new();
        let mut props = PropertySet::new();

        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();

        let mut dag = circuit.into_dag();
        pm.run(&mut dag, &mut props).unwrap();

        assert_eq!(dag.num_ops(), 2);
    }

    #[test]
    fn test_pass_manager_builder() {
        let (pm, props) = PassManagerBuilder::new()
            .with_optimization_level(2)
            .with_target(CouplingMap::linear(5), BasisGates::iqm())
            .build();

        assert!(!pm.is_empty());
        assert!(props.coupling_map.is_some());
        assert!(props.basis_gates.is_some());
    }
}
