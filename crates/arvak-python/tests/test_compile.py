"""Tests for the hiq compilation types."""

import pytest

from arvak import Layout, CouplingMap, BasisGates, PropertySet, QubitId


class TestLayout:
    """Test Layout class."""

    def test_create_empty_layout(self):
        """Test creating an empty layout."""
        layout = Layout()
        assert len(layout) == 0

    def test_trivial_layout(self):
        """Test creating a trivial layout."""
        layout = Layout.trivial(5)
        assert len(layout) == 5
        assert layout.get_physical(0) == 0
        assert layout.get_physical(4) == 4

    def test_add_mapping(self):
        """Test adding mappings to a layout."""
        layout = Layout()
        layout.add(0, 3)
        layout.add(1, 2)
        assert layout.get_physical(0) == 3
        assert layout.get_physical(1) == 2

    def test_get_logical(self):
        """Test getting logical qubit from physical."""
        layout = Layout.trivial(3)
        logical = layout.get_logical(1)
        assert logical is not None
        assert logical.index == 1

    def test_swap(self):
        """Test swapping physical qubits."""
        layout = Layout.trivial(3)
        layout.swap(0, 2)
        assert layout.get_physical(0) == 2
        assert layout.get_physical(2) == 0


class TestCouplingMap:
    """Test CouplingMap class."""

    def test_create_empty_map(self):
        """Test creating an empty coupling map."""
        cm = CouplingMap(5)
        assert cm.num_qubits == 5
        assert len(cm.edges()) == 0

    def test_add_edge(self):
        """Test adding edges."""
        cm = CouplingMap(3)
        cm.add_edge(0, 1)
        cm.add_edge(1, 2)
        assert cm.is_connected(0, 1)
        assert cm.is_connected(1, 0)  # Bidirectional
        assert cm.is_connected(1, 2)
        assert not cm.is_connected(0, 2)

    def test_linear_topology(self):
        """Test linear topology."""
        cm = CouplingMap.linear(5)
        assert cm.num_qubits == 5
        assert cm.is_connected(0, 1)
        assert cm.is_connected(3, 4)
        assert not cm.is_connected(0, 2)

    def test_star_topology(self):
        """Test star topology."""
        cm = CouplingMap.star(5)
        assert cm.is_connected(0, 1)
        assert cm.is_connected(0, 4)
        assert not cm.is_connected(1, 2)

    def test_full_topology(self):
        """Test fully connected topology."""
        cm = CouplingMap.full(4)
        for i in range(4):
            for j in range(4):
                if i != j:
                    assert cm.is_connected(i, j)

    def test_distance(self):
        """Test shortest path distance."""
        cm = CouplingMap.linear(5)
        assert cm.distance(0, 0) == 0
        assert cm.distance(0, 1) == 1
        assert cm.distance(0, 4) == 4


class TestBasisGates:
    """Test BasisGates class."""

    def test_create_custom_basis(self):
        """Test creating custom basis gates."""
        basis = BasisGates(["h", "cx", "rz"])
        assert basis.contains("h")
        assert basis.contains("cx")
        assert not basis.contains("rx")

    def test_iqm_basis(self):
        """Test IQM basis gates."""
        basis = BasisGates.iqm()
        assert basis.contains("prx")
        assert basis.contains("cz")
        assert not basis.contains("cx")

    def test_ibm_basis(self):
        """Test IBM basis gates."""
        basis = BasisGates.ibm()
        assert basis.contains("rz")
        assert basis.contains("sx")
        assert basis.contains("cx")
        assert not basis.contains("prx")

    def test_universal_basis(self):
        """Test universal basis gates."""
        basis = BasisGates.universal()
        assert basis.contains("h")
        assert basis.contains("cx")
        assert basis.contains("prx")
        assert basis.contains("measure")

    def test_get_gates(self):
        """Test getting list of gates."""
        basis = BasisGates(["a", "b", "c"])
        gates = basis.gates()
        assert len(gates) == 3
        assert "a" in gates


class TestPropertySet:
    """Test PropertySet class."""

    def test_create_empty_property_set(self):
        """Test creating an empty property set."""
        props = PropertySet()
        assert props.get_layout() is None
        assert props.get_coupling_map() is None
        assert props.get_basis_gates() is None

    def test_set_layout(self):
        """Test setting layout."""
        props = PropertySet()
        layout = Layout.trivial(3)
        props.set_layout(layout)

        retrieved = props.get_layout()
        assert retrieved is not None
        assert len(retrieved) == 3

    def test_set_coupling_map(self):
        """Test setting coupling map."""
        props = PropertySet()
        cm = CouplingMap.linear(5)
        props.set_coupling_map(cm)

        retrieved = props.get_coupling_map()
        assert retrieved is not None
        assert retrieved.num_qubits == 5

    def test_set_basis_gates(self):
        """Test setting basis gates."""
        props = PropertySet()
        basis = BasisGates.iqm()
        props.set_basis_gates(basis)

        retrieved = props.get_basis_gates()
        assert retrieved is not None
        assert retrieved.contains("prx")

    def test_with_target_fluent(self):
        """Test fluent with_target method."""
        props = PropertySet()
        props.with_target(CouplingMap.star(5), BasisGates.ibm())

        assert props.get_coupling_map() is not None
        assert props.get_basis_gates() is not None


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
