// Arvak Dashboard Frontend Application

// ============================================================================
// State Management
// ============================================================================

const state = {
    currentView: 'circuits',
    circuit: null,
    compileResult: null,
    backends: [],
    jobs: [],
};

// ============================================================================
// API Client
// ============================================================================

const api = {
    async health() {
        const res = await fetch('/api/health');
        return res.json();
    },

    async visualizeCircuit(qasm) {
        const res = await fetch('/api/circuits/visualize', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ qasm }),
        });
        if (!res.ok) {
            const error = await res.json();
            throw new Error(error.message || 'Failed to visualize circuit');
        }
        return res.json();
    },

    async compileCircuit(qasm, target, optimizationLevel) {
        const res = await fetch('/api/circuits/compile', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                qasm,
                target,
                optimization_level: parseInt(optimizationLevel, 10)
            }),
        });
        if (!res.ok) {
            const error = await res.json();
            throw new Error(error.message || 'Failed to compile circuit');
        }
        return res.json();
    },

    async listBackends() {
        const res = await fetch('/api/backends');
        return res.json();
    },

    async getBackend(name) {
        const res = await fetch(`/api/backends/${encodeURIComponent(name)}`);
        return res.json();
    },

    // Job management
    async listJobs(params = {}) {
        const query = new URLSearchParams();
        if (params.status) query.set('status', params.status);
        if (params.limit) query.set('limit', params.limit);
        if (params.pending) query.set('pending', 'true');
        if (params.running) query.set('running', 'true');

        const url = '/api/jobs' + (query.toString() ? '?' + query.toString() : '');
        const res = await fetch(url);
        if (!res.ok) {
            const error = await res.json().catch(() => ({}));
            throw new Error(error.message || 'Failed to load jobs');
        }
        return res.json();
    },

    async getJob(id) {
        const res = await fetch(`/api/jobs/${encodeURIComponent(id)}`);
        if (!res.ok) {
            const error = await res.json();
            throw new Error(error.message || 'Failed to get job');
        }
        return res.json();
    },

    async createJob(job) {
        const res = await fetch('/api/jobs', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(job),
        });
        if (!res.ok) {
            const error = await res.json();
            throw new Error(error.message || 'Failed to create job');
        }
        return res.json();
    },

    async deleteJob(id) {
        const res = await fetch(`/api/jobs/${encodeURIComponent(id)}`, {
            method: 'DELETE',
        });
        if (!res.ok) {
            const error = await res.json();
            throw new Error(error.message || 'Failed to delete job');
        }
        return res.json();
    },

    async getJobResult(id) {
        const res = await fetch(`/api/jobs/${encodeURIComponent(id)}/result`);
        if (!res.ok) {
            const error = await res.json();
            throw new Error(error.message || 'Failed to get job result');
        }
        return res.json();
    },

    async evaluate(params) {
        const res = await fetch('/api/eval', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(params),
        });
        if (!res.ok) {
            const error = await res.json().catch(() => ({}));
            throw new Error(error.message || 'Evaluation failed');
        }
        return res.json();
    },

    async getVqeDemo() {
        const res = await fetch('/api/vqe/demo');
        if (!res.ok) {
            const error = await res.json().catch(() => ({}));
            throw new Error(error.message || 'Failed to load VQE demo');
        }
        return res.json();
    },
};

// ============================================================================
// Circuit Visualization (D3.js)
// ============================================================================

const circuitRenderer = {
    // Layout constants
    WIRE_SPACING: 40,
    LAYER_WIDTH: 60,
    GATE_WIDTH: 40,
    GATE_HEIGHT: 30,
    MARGIN: { top: 20, right: 40, bottom: 20, left: 60 },

    render(container, circuit, options = {}) {
        // Clear previous content
        d3.select(container).selectAll('*').remove();

        if (!circuit || !circuit.layers || circuit.num_qubits === 0) {
            d3.select(container)
                .append('p')
                .attr('class', 'placeholder')
                .text('No circuit to display');
            return;
        }

        const numQubits = circuit.num_qubits;
        const numLayers = circuit.layers.length;

        // Calculate dimensions
        const width = this.MARGIN.left + this.MARGIN.right + numLayers * this.LAYER_WIDTH + 40;
        const height = this.MARGIN.top + this.MARGIN.bottom + numQubits * this.WIRE_SPACING;

        // Create SVG
        const svg = d3.select(container)
            .append('svg')
            .attr('width', width)
            .attr('height', height)
            .attr('class', 'circuit-svg');

        const g = svg.append('g')
            .attr('transform', `translate(${this.MARGIN.left}, ${this.MARGIN.top})`);

        // Draw wires
        this.drawWires(g, numQubits, numLayers);

        // Draw gates layer by layer
        circuit.layers.forEach((layer, layerIdx) => {
            layer.operations.forEach(op => {
                this.drawOperation(g, op, layerIdx);
            });
        });
    },

    drawWires(g, numQubits, numLayers) {
        const wireLength = numLayers * this.LAYER_WIDTH + 20;

        for (let i = 0; i < numQubits; i++) {
            const y = i * this.WIRE_SPACING + this.WIRE_SPACING / 2;

            // Wire line
            g.append('line')
                .attr('class', 'wire')
                .attr('x1', 0)
                .attr('y1', y)
                .attr('x2', wireLength)
                .attr('y2', y);

            // Wire label
            g.append('text')
                .attr('class', 'wire-label')
                .attr('x', -10)
                .attr('y', y)
                .attr('text-anchor', 'end')
                .attr('dominant-baseline', 'middle')
                .text(`q[${i}]`);
        }
    },

    drawOperation(g, op, layerIdx) {
        const x = layerIdx * this.LAYER_WIDTH + this.LAYER_WIDTH / 2;

        if (op.num_qubits === 1) {
            this.drawSingleQubitGate(g, op, x);
        } else if (op.num_qubits === 2) {
            this.drawTwoQubitGate(g, op, x);
        } else {
            this.drawMultiQubitGate(g, op, x);
        }
    },

    drawSingleQubitGate(g, op, x) {
        const qubit = op.qubits[0];
        const y = qubit * this.WIRE_SPACING + this.WIRE_SPACING / 2;

        let boxClass = 'gate-box';
        if (op.is_measurement) {
            boxClass += ' measurement';
        }

        // Gate box
        g.append('rect')
            .attr('class', boxClass)
            .attr('x', x - this.GATE_WIDTH / 2)
            .attr('y', y - this.GATE_HEIGHT / 2)
            .attr('width', this.GATE_WIDTH)
            .attr('height', this.GATE_HEIGHT);

        // Gate label
        g.append('text')
            .attr('class', 'gate-label')
            .attr('x', x)
            .attr('y', y)
            .text(this.truncateLabel(op.label));
    },

    drawTwoQubitGate(g, op, x) {
        const q0 = Math.min(...op.qubits);
        const q1 = Math.max(...op.qubits);
        const y0 = q0 * this.WIRE_SPACING + this.WIRE_SPACING / 2;
        const y1 = q1 * this.WIRE_SPACING + this.WIRE_SPACING / 2;

        // Check if it's a controlled gate (CX, CY, CZ, etc.)
        const isControlled = op.gate.startsWith('c') && op.gate !== 'cswap';

        if (isControlled) {
            // Draw control dot on first qubit
            g.append('circle')
                .attr('class', 'control-dot')
                .attr('cx', x)
                .attr('cy', y0)
                .attr('r', 5);

            // Draw connector line
            g.append('line')
                .attr('class', 'gate-connector')
                .attr('x1', x)
                .attr('y1', y0)
                .attr('x2', x)
                .attr('y2', y1);

            // Draw target (circle with plus for CX, box for others)
            if (op.gate === 'cx') {
                // XOR symbol (circle with plus)
                g.append('circle')
                    .attr('class', 'target-circle')
                    .attr('cx', x)
                    .attr('cy', y1)
                    .attr('r', 12);

                g.append('line')
                    .attr('class', 'gate-connector')
                    .attr('x1', x)
                    .attr('y1', y1 - 12)
                    .attr('x2', x)
                    .attr('y2', y1 + 12);

                g.append('line')
                    .attr('class', 'gate-connector')
                    .attr('x1', x - 12)
                    .attr('y1', y1)
                    .attr('x2', x + 12)
                    .attr('y2', y1);
            } else {
                // Box for other controlled gates
                g.append('rect')
                    .attr('class', 'gate-box two-qubit')
                    .attr('x', x - this.GATE_WIDTH / 2)
                    .attr('y', y1 - this.GATE_HEIGHT / 2)
                    .attr('width', this.GATE_WIDTH)
                    .attr('height', this.GATE_HEIGHT);

                g.append('text')
                    .attr('class', 'gate-label')
                    .attr('x', x)
                    .attr('y', y1)
                    .text(this.truncateLabel(op.label.replace(/^C/, '')));
            }
        } else {
            // SWAP or other two-qubit gates: draw box spanning both qubits
            const boxHeight = (q1 - q0) * this.WIRE_SPACING + this.GATE_HEIGHT;
            const boxY = y0 - this.GATE_HEIGHT / 2;

            g.append('rect')
                .attr('class', 'gate-box two-qubit')
                .attr('x', x - this.GATE_WIDTH / 2)
                .attr('y', boxY)
                .attr('width', this.GATE_WIDTH)
                .attr('height', boxHeight);

            g.append('text')
                .attr('class', 'gate-label')
                .attr('x', x)
                .attr('y', (y0 + y1) / 2)
                .text(this.truncateLabel(op.label));
        }
    },

    drawMultiQubitGate(g, op, x) {
        // For 3+ qubit gates, draw a box spanning all qubits
        const qubits = [...op.qubits].sort((a, b) => a - b);
        const q0 = qubits[0];
        const q1 = qubits[qubits.length - 1];
        const y0 = q0 * this.WIRE_SPACING + this.WIRE_SPACING / 2;
        const y1 = q1 * this.WIRE_SPACING + this.WIRE_SPACING / 2;

        const boxHeight = (q1 - q0) * this.WIRE_SPACING + this.GATE_HEIGHT;
        const boxY = y0 - this.GATE_HEIGHT / 2;

        g.append('rect')
            .attr('class', 'gate-box two-qubit')
            .attr('x', x - this.GATE_WIDTH / 2)
            .attr('y', boxY)
            .attr('width', this.GATE_WIDTH)
            .attr('height', boxHeight);

        g.append('text')
            .attr('class', 'gate-label')
            .attr('x', x)
            .attr('y', (y0 + y1) / 2)
            .text(this.truncateLabel(op.label));
    },

    truncateLabel(label) {
        // Truncate long labels for display
        if (label.length > 6) {
            return label.substring(0, 5) + '...';
        }
        return label;
    },
};

// ============================================================================
// Topology Renderer (D3.js)
// ============================================================================

const topologyRenderer = {
    render(container, topology, options = {}) {
        d3.select(container).selectAll('*').remove();

        if (!topology || !topology.edges || topology.num_qubits === 0) {
            d3.select(container)
                .append('p')
                .attr('class', 'placeholder')
                .text('No topology data');
            return;
        }

        const title = options.title || 'Device Topology';
        d3.select(container).append('h4').text(title);

        const n = topology.num_qubits;
        const edges = topology.edges;
        const mappedQubits = options.mappedQubits || {};  // { physical: logical }
        const usedEdges = options.usedEdges || new Set();  // Set of "q1-q2" strings

        const margin = 20;
        const size = Math.min(container.clientWidth || 300, 400) - margin * 2;
        const radius = size / 2 - 20;
        const cx = size / 2 + margin;
        const cy = size / 2 + margin;

        const svg = d3.select(container)
            .append('svg')
            .attr('width', size + margin * 2)
            .attr('height', size + margin * 2);

        // Compute node positions based on topology kind
        const positions = this.computePositions(topology.kind, n, cx, cy, radius);

        // Draw edges
        edges.forEach(([q1, q2]) => {
            const p1 = positions[q1];
            const p2 = positions[q2];
            if (!p1 || !p2) return;

            const edgeKey = `${Math.min(q1, q2)}-${Math.max(q1, q2)}`;
            svg.append('line')
                .attr('class', `topology-edge${usedEdges.has(edgeKey) ? ' used' : ''}`)
                .attr('x1', p1.x).attr('y1', p1.y)
                .attr('x2', p2.x).attr('y2', p2.y);
        });

        // Color scale for mapped qubits
        const qubitColors = d3.scaleOrdinal(d3.schemeTableau10);

        // Draw nodes
        for (let i = 0; i < n; i++) {
            const p = positions[i];
            if (!p) continue;

            const isMapped = i in mappedQubits;
            const nodeColor = isMapped ? qubitColors(mappedQubits[i]) : null;

            const circle = svg.append('circle')
                .attr('class', `topology-node${isMapped ? ' mapped' : ''}`)
                .attr('cx', p.x)
                .attr('cy', p.y)
                .attr('r', 14);

            if (nodeColor) {
                circle.attr('stroke', nodeColor);
            }

            svg.append('text')
                .attr('class', 'topology-node-label')
                .attr('x', p.x)
                .attr('y', p.y)
                .text(i);
        }
    },

    computePositions(kind, n, cx, cy, radius) {
        const positions = {};

        if (kind === 'linear') {
            // Horizontal chain
            const spacing = (radius * 2) / Math.max(n - 1, 1);
            const startX = cx - radius;
            for (let i = 0; i < n; i++) {
                positions[i] = { x: startX + i * spacing, y: cy };
            }
        } else if (kind === 'star') {
            // Center node + radial
            positions[0] = { x: cx, y: cy };
            for (let i = 1; i < n; i++) {
                const angle = ((i - 1) / (n - 1)) * 2 * Math.PI - Math.PI / 2;
                positions[i] = {
                    x: cx + radius * Math.cos(angle),
                    y: cy + radius * Math.sin(angle),
                };
            }
        } else if (kind === 'grid') {
            const cols = Math.ceil(Math.sqrt(n));
            const rows = Math.ceil(n / cols);
            const spacingX = (radius * 2) / Math.max(cols - 1, 1);
            const spacingY = (radius * 2) / Math.max(rows - 1, 1);
            const startX = cx - radius;
            const startY = cy - radius;
            for (let i = 0; i < n; i++) {
                const row = Math.floor(i / cols);
                const col = i % cols;
                positions[i] = { x: startX + col * spacingX, y: startY + row * spacingY };
            }
        } else {
            // Fully connected / unknown → circular layout
            for (let i = 0; i < n; i++) {
                const angle = (i / n) * 2 * Math.PI - Math.PI / 2;
                positions[i] = {
                    x: cx + radius * Math.cos(angle),
                    y: cy + radius * Math.sin(angle),
                };
            }
        }

        return positions;
    },
};

// ============================================================================
// ESP Chart Renderer (D3.js)
// ============================================================================

const espChartRenderer = {
    render(container, espData) {
        d3.select(container).selectAll('*').remove();

        if (!espData || !espData.layer_esp || espData.layer_esp.length === 0) {
            d3.select(container)
                .append('p')
                .attr('class', 'placeholder')
                .text('No ESP data');
            return;
        }

        // Title
        d3.select(container).append('h4').text('Estimated Success Probability');

        // ESP badge
        const totalEsp = espData.total_esp;
        const espPercent = (totalEsp * 100).toFixed(1);
        let badgeClass = 'esp-badge';
        if (totalEsp >= 0.9) badgeClass += ' high';
        else if (totalEsp >= 0.5) badgeClass += ' medium';
        else badgeClass += ' low';

        d3.select(container).append('div')
            .attr('class', badgeClass)
            .text(`${espPercent}%`);

        // Chart
        const margin = { top: 10, right: 20, bottom: 40, left: 50 };
        const width = Math.min(container.clientWidth || 500, 700) - margin.left - margin.right;
        const height = 160 - margin.top - margin.bottom;

        const svg = d3.select(container)
            .append('svg')
            .attr('width', width + margin.left + margin.right)
            .attr('height', height + margin.top + margin.bottom)
            .append('g')
            .attr('transform', `translate(${margin.left},${margin.top})`);

        const n = espData.layer_esp.length;

        // X scale
        const x = d3.scaleBand()
            .domain(d3.range(n).map(String))
            .range([0, width])
            .padding(0.15);

        // Y scale
        const y = d3.scaleLinear()
            .domain([0, 1])
            .range([height, 0]);

        // Per-layer ESP bars
        svg.selectAll('.esp-bar')
            .data(espData.layer_esp)
            .enter()
            .append('rect')
            .attr('class', 'esp-bar')
            .attr('x', (d, i) => x(String(i)))
            .attr('y', d => y(d))
            .attr('width', x.bandwidth())
            .attr('height', d => height - y(d));

        // Cumulative ESP line
        const lineGen = d3.line()
            .x((d, i) => x(String(i)) + x.bandwidth() / 2)
            .y(d => y(d));

        svg.append('path')
            .datum(espData.cumulative_esp)
            .attr('class', 'esp-line')
            .attr('d', lineGen);

        // Dots on cumulative line
        svg.selectAll('.esp-dot')
            .data(espData.cumulative_esp)
            .enter()
            .append('circle')
            .attr('class', 'esp-dot')
            .attr('cx', (d, i) => x(String(i)) + x.bandwidth() / 2)
            .attr('cy', d => y(d))
            .attr('r', 3);

        // X axis
        svg.append('g')
            .attr('class', 'axis')
            .attr('transform', `translate(0,${height})`)
            .call(d3.axisBottom(x).tickValues(
                n <= 20 ? d3.range(n).map(String) : d3.range(0, n, Math.ceil(n / 10)).map(String)
            ));

        // X label
        svg.append('text')
            .attr('class', 'axis-label')
            .attr('x', width / 2)
            .attr('y', height + margin.bottom - 5)
            .style('text-anchor', 'middle')
            .text('Layer');

        // Y axis
        svg.append('g')
            .attr('class', 'axis')
            .call(d3.axisLeft(y).ticks(4).tickFormat(d => `${(d * 100).toFixed(0)}%`));
    },
};

// ============================================================================
// Qubit Mapping Renderer (D3.js)
// ============================================================================

const mappingRenderer = {
    render(container, mapping) {
        d3.select(container).selectAll('*').remove();

        if (!mapping || !mapping.mappings || mapping.mappings.length === 0) {
            return;  // Don't show anything if no mapping
        }

        d3.select(container).append('h4').text('Qubit Map');

        const entries = mapping.mappings;
        const n = entries.length;
        const rowHeight = 28;
        const width = 130;
        const height = n * rowHeight + 20;
        const colLeft = 10;
        const colRight = width - 10;

        const colors = d3.scaleOrdinal(d3.schemeTableau10);

        const svg = d3.select(container)
            .append('svg')
            .attr('width', width)
            .attr('height', height);

        entries.forEach((entry, i) => {
            const yLeft = i * rowHeight + 20;
            // Physical qubits may be in different order — find vertical position
            const physIdx = entries.findIndex(e => e.physical === i);
            const yRight = (physIdx >= 0 ? physIdx : i) * rowHeight + 20;

            const color = colors(entry.logical);

            // Curved connecting line
            const midX = width / 2;
            svg.append('path')
                .attr('class', 'mapping-line')
                .attr('d', `M ${colLeft + 25} ${yLeft} C ${midX} ${yLeft}, ${midX} ${yRight}, ${colRight - 25} ${yRight}`)
                .attr('stroke', color);

            // Left label (logical)
            svg.append('text')
                .attr('class', 'mapping-label')
                .attr('x', colLeft)
                .attr('y', yLeft)
                .attr('dominant-baseline', 'middle')
                .attr('fill', color)
                .text(`q[${entry.logical}]`);

            // Right label (physical)
            svg.append('text')
                .attr('class', 'mapping-label')
                .attr('x', colRight)
                .attr('y', yRight)
                .attr('text-anchor', 'end')
                .attr('dominant-baseline', 'middle')
                .attr('fill', color)
                .text(`p[${entry.physical}]`);
        });
    },
};

// ============================================================================
// View Controllers
// ============================================================================

function showView(viewName) {
    state.currentView = viewName;

    // Update hash without triggering hashchange
    history.replaceState(null, '', '#' + viewName);

    // Update nav
    document.querySelectorAll('nav a').forEach(a => {
        a.classList.toggle('active', a.dataset.view === viewName);
    });

    // Update views
    document.querySelectorAll('.view').forEach(v => {
        v.classList.toggle('active', v.id === `${viewName}-view`);
    });

    // Load data for the view
    if (viewName === 'backends') {
        loadBackends();
    } else if (viewName === 'jobs') {
        loadJobs();
    } else if (viewName === 'vqe') {
        loadVqe();
    } else if (viewName === 'eval') {
        // Eval view loads on demand via button
    } else if (viewName === 'nathan') {
        initNathan();
    }
}

async function visualizeCircuit() {
    const qasm = document.getElementById('qasm-input').value;
    const container = document.getElementById('circuit-container');
    const infoBar = document.getElementById('circuit-info');

    if (!qasm.trim()) {
        showError(container, 'Please enter QASM3 code');
        return;
    }

    try {
        container.innerHTML = '<p class="placeholder">Loading...</p>';
        infoBar.innerHTML = '';

        // Hide compile results when just visualizing
        document.getElementById('compile-results').style.display = 'none';

        const circuit = await api.visualizeCircuit(qasm);
        state.circuit = circuit;

        // Update info bar
        infoBar.innerHTML = `
            <span><span class="label">Name:</span> <span class="value">${escapeHtml(String(circuit.name))}</span></span>
            <span><span class="label">Qubits:</span> <span class="value">${escapeHtml(String(circuit.num_qubits))}</span></span>
            <span><span class="label">Depth:</span> <span class="value">${escapeHtml(String(circuit.depth))}</span></span>
            <span><span class="label">Gates:</span> <span class="value">${escapeHtml(String(circuit.num_ops))}</span></span>
        `;

        // Render circuit
        circuitRenderer.render(container, circuit);
    } catch (error) {
        showError(container, error.message);
    }
}

async function compileCircuit() {
    const qasm = document.getElementById('qasm-input').value;
    const target = document.getElementById('target-select').value;
    const optLevel = document.getElementById('opt-level').value;
    const container = document.getElementById('circuit-container');
    const infoBar = document.getElementById('circuit-info');
    const resultsPanel = document.getElementById('compile-results');

    if (!qasm.trim()) {
        showError(container, 'Please enter QASM3 code');
        return;
    }

    if (!target) {
        showError(container, 'Please select a compilation target');
        return;
    }

    try {
        container.innerHTML = '<p class="placeholder">Compiling...</p>';
        infoBar.innerHTML = '';

        const result = await api.compileCircuit(qasm, target, optLevel);
        state.compileResult = result;

        // Update main circuit info
        infoBar.innerHTML = `
            <span><span class="label">Name:</span> <span class="value">${escapeHtml(String(result.before.name))}</span></span>
            <span><span class="label">Target:</span> <span class="value">${escapeHtml(target.toUpperCase())}</span></span>
            <span><span class="label">Opt Level:</span> <span class="value">${escapeHtml(String(optLevel))}</span></span>
        `;

        // Render original circuit in main container
        circuitRenderer.render(container, result.before);

        // Show compilation results
        resultsPanel.style.display = 'block';

        // Update stats
        const stats = result.stats;
        const depthChange = stats.compiled_depth - stats.original_depth;
        const gateChange = stats.gates_after - stats.gates_before;

        const timeStr = stats.compile_time_us < 1000
            ? `${stats.compile_time_us}\u00B5s`
            : `${(stats.compile_time_us / 1000).toFixed(2)}ms`;
        const throughputStr = stats.throughput_gates_per_sec >= 1000000
            ? `${(stats.throughput_gates_per_sec / 1000000).toFixed(1)}M gates/s`
            : `${(stats.throughput_gates_per_sec / 1000).toFixed(0)}K gates/s`;

        document.getElementById('compile-stats').innerHTML = `
            <span><span class="label">Original Depth:</span> <span class="value">${escapeHtml(String(stats.original_depth))}</span></span>
            <span><span class="label">Compiled Depth:</span> <span class="value ${depthChange < 0 ? 'improved' : depthChange > 0 ? 'degraded' : ''}">${escapeHtml(String(stats.compiled_depth))} (${depthChange >= 0 ? '+' : ''}${escapeHtml(String(depthChange))})</span></span>
            <span><span class="label">Original Gates:</span> <span class="value">${escapeHtml(String(stats.gates_before))}</span></span>
            <span><span class="label">Compiled Gates:</span> <span class="value ${gateChange < 0 ? 'improved' : gateChange > 0 ? 'degraded' : ''}">${escapeHtml(String(stats.gates_after))} (${gateChange >= 0 ? '+' : ''}${escapeHtml(String(gateChange))})</span></span>
            <span><span class="label">Compile Time:</span> <span class="value improved">${escapeHtml(String(timeStr))}</span></span>
            <span><span class="label">Throughput:</span> <span class="value improved">${escapeHtml(String(throughputStr))}</span></span>
        `;

        // Render before/after circuits
        circuitRenderer.render(document.getElementById('circuit-before'), result.before);
        circuitRenderer.render(document.getElementById('circuit-after'), result.after);

        // Render qubit mapping
        if (result.qubit_mapping) {
            mappingRenderer.render(
                document.getElementById('qubit-mapping-container'),
                result.qubit_mapping
            );
        }

        // Render ESP chart
        if (result.esp) {
            espChartRenderer.render(
                document.getElementById('esp-chart-container'),
                result.esp
            );
        }

        // Render compile topology
        if (result.topology && result.qubit_mapping) {
            const mappedQubits = {};
            result.qubit_mapping.mappings.forEach(m => {
                mappedQubits[m.physical] = m.logical;
            });

            // Find used edges from 2-qubit gates in compiled circuit
            const usedEdges = new Set();
            if (result.after && result.after.layers) {
                result.after.layers.forEach(layer => {
                    layer.operations.forEach(op => {
                        if (op.num_qubits >= 2 && op.qubits.length >= 2) {
                            const q1 = Math.min(op.qubits[0], op.qubits[1]);
                            const q2 = Math.max(op.qubits[0], op.qubits[1]);
                            usedEdges.add(`${q1}-${q2}`);
                        }
                    });
                });
            }

            topologyRenderer.render(
                document.getElementById('compile-topology-container'),
                result.topology,
                { title: `${target.toUpperCase()} Topology`, mappedQubits, usedEdges }
            );
        }

        // Show compiled QASM
        document.getElementById('compiled-qasm').value = result.compiled_qasm;

    } catch (error) {
        showError(container, error.message);
        resultsPanel.style.display = 'none';
    }
}

async function loadBackends() {
    const container = document.getElementById('backends-container');

    try {
        container.innerHTML = '<p class="placeholder">Loading backends...</p>';

        const backends = await api.listBackends();
        state.backends = backends;

        if (backends.length === 0) {
            container.innerHTML = '<p class="placeholder">No backends configured</p>';
            return;
        }

        const grid = document.createElement('div');
        grid.className = 'backend-grid';

        backends.forEach(backend => {
            const card = document.createElement('div');
            card.className = 'backend-card';
            card.style.cursor = 'pointer';
            card.innerHTML = `
                <h3>
                    ${escapeHtml(backend.name)}
                    <span class="status ${backend.available ? 'available' : 'unavailable'}"></span>
                </h3>
                <div class="info">
                    <span><strong>Type:</strong> ${backend.is_simulator ? 'Simulator' : 'Hardware'}</span>
                    <span><strong>Qubits:</strong> ${escapeHtml(String(backend.num_qubits))}</span>
                    <span><strong>Status:</strong> ${backend.available ? 'Available' : 'Unavailable'}</span>
                </div>
                <div class="gates">
                    <strong>Native gates:</strong>
                    ${backend.native_gates.length > 0
                        ? backend.native_gates.map(g => `<span class="tag">${escapeHtml(g)}</span>`).join('')
                        : '<span class="tag">universal</span>'}
                </div>
            `;
            card.addEventListener('click', () => showBackendTopology(backend.name));
            grid.appendChild(card);
        });

        container.innerHTML = '';
        container.appendChild(grid);
    } catch (error) {
        showError(container, error.message);
    }
}

async function showBackendTopology(name) {
    const detailContainer = document.getElementById('backend-detail-container');
    const topoContainer = document.getElementById('backend-topology-container');

    try {
        const details = await api.getBackend(name);
        document.getElementById('backend-detail-name').textContent = `${name} — ${details.num_qubits} qubits`;
        detailContainer.style.display = 'block';

        if (details.topology) {
            topologyRenderer.render(topoContainer, details.topology, {
                title: `${details.topology.kind} topology`,
            });
        } else {
            topoContainer.innerHTML = '<p class="placeholder">No topology information</p>';
        }
    } catch (error) {
        detailContainer.style.display = 'block';
        topoContainer.innerHTML = `<div class="error-message">${escapeHtml(error.message)}</div>`;
    }
}

function showError(container, message) {
    container.innerHTML = `<div class="error-message">${escapeHtml(message)}</div>`;
}

// ============================================================================
// Jobs View Controller
// ============================================================================

async function loadJobs() {
    const container = document.getElementById('jobs-container');

    try {
        container.innerHTML = '<p class="placeholder">Loading jobs...</p>';

        const jobs = await api.listJobs({ limit: 50 });
        state.jobs = jobs;

        if (jobs.length === 0) {
            container.innerHTML = `
                <p class="placeholder">No jobs found</p>
                <p class="placeholder small">Jobs will appear here when submitted through the scheduler.</p>
            `;
            return;
        }

        renderJobsTable(container, jobs);
    } catch (error) {
        showError(container, error.message);
    }
}

function renderJobsTable(container, jobs) {
    const table = document.createElement('table');
    table.className = 'jobs-table';
    table.innerHTML = `
        <thead>
            <tr>
                <th>ID</th>
                <th>Name</th>
                <th>Status</th>
                <th>Backend</th>
                <th>Shots</th>
                <th>Priority</th>
                <th>Created</th>
                <th>Actions</th>
            </tr>
        </thead>
        <tbody>
            ${jobs.map(job => `
                <tr class="job-row" data-job-id="${escapeHtml(job.id)}">
                    <td class="job-id" title="${escapeHtml(job.id)}">${escapeHtml(job.id.substring(0, 8))}...</td>
                    <td class="job-name">${escapeHtml(job.name)}</td>
                    <td>
                        <span class="status-badge status-${escapeHtml(job.status.toLowerCase())}">${escapeHtml(job.status)}</span>
                        ${job.status_details ? `<span class="status-details" title="${escapeHtml(job.status_details)}">ℹ️</span>` : ''}
                    </td>
                    <td>${escapeHtml(job.backend || '-')}</td>
                    <td>${escapeHtml(String(job.shots))}</td>
                    <td>${escapeHtml(String(job.priority))}</td>
                    <td class="job-time">${formatTime(job.created_at)}</td>
                    <td class="job-actions">
                        <button class="btn-small" data-action="view" data-job-id="${escapeHtml(job.id)}">View</button>
                        ${isJobCancellable(job.status) ? `<button class="btn-small btn-danger" data-action="cancel" data-job-id="${escapeHtml(job.id)}">Cancel</button>` : ''}
                    </td>
                </tr>
            `).join('')}
        </tbody>
    `;

    container.innerHTML = '';
    container.appendChild(table);

    // Event delegation for job action buttons
    table.addEventListener('click', (e) => {
        const btn = e.target.closest('button[data-action]');
        if (!btn) return;
        const jobId = btn.dataset.jobId;
        if (btn.dataset.action === 'view') {
            viewJobDetails(jobId);
        } else if (btn.dataset.action === 'cancel') {
            cancelJob(jobId);
        }
    });
}

function isJobCancellable(status) {
    return ['pending', 'queued', 'running', 'slurm_queued', 'slurm_running', 'quantum_submitted', 'quantum_running'].includes(status.toLowerCase());
}

async function viewJobDetails(jobId) {
    const container = document.getElementById('jobs-container');

    try {
        container.innerHTML = '<p class="placeholder">Loading job details...</p>';

        const job = await api.getJob(jobId);

        const detailsHtml = `
            <div class="job-details-panel">
                <div class="job-details-header">
                    <button class="btn-back" onclick="loadJobs()">← Back to Jobs</button>
                    <h3>${escapeHtml(job.name)}</h3>
                    <span class="status-badge status-${escapeHtml(job.status.toLowerCase())}">${escapeHtml(job.status)}</span>
                </div>

                <div class="job-details-grid">
                    <div class="detail-item">
                        <span class="label">Job ID</span>
                        <span class="value">${escapeHtml(job.id)}</span>
                    </div>
                    <div class="detail-item">
                        <span class="label">Status</span>
                        <span class="value">${escapeHtml(job.status)}${job.status_details ? ' - ' + escapeHtml(job.status_details) : ''}</span>
                    </div>
                    <div class="detail-item">
                        <span class="label">Backend</span>
                        <span class="value">${escapeHtml(job.backend || 'Not assigned')}</span>
                    </div>
                    <div class="detail-item">
                        <span class="label">Shots</span>
                        <span class="value">${escapeHtml(String(job.shots))}</span>
                    </div>
                    <div class="detail-item">
                        <span class="label">Priority</span>
                        <span class="value">${escapeHtml(String(job.priority))}</span>
                    </div>
                    <div class="detail-item">
                        <span class="label">Circuits</span>
                        <span class="value">${escapeHtml(String(job.num_circuits))}</span>
                    </div>
                    <div class="detail-item">
                        <span class="label">Created</span>
                        <span class="value">${formatTime(job.created_at)}</span>
                    </div>
                    ${job.submitted_at ? `
                    <div class="detail-item">
                        <span class="label">Submitted</span>
                        <span class="value">${formatTime(job.submitted_at)}</span>
                    </div>` : ''}
                    ${job.completed_at ? `
                    <div class="detail-item">
                        <span class="label">Completed</span>
                        <span class="value">${formatTime(job.completed_at)}</span>
                    </div>` : ''}
                </div>

                ${job.qasm ? `
                <div class="job-qasm">
                    <h4>Circuit QASM</h4>
                    <textarea readonly rows="10">${escapeHtml(job.qasm)}</textarea>
                </div>` : ''}

                ${Object.keys(job.metadata || {}).length > 0 ? `
                <div class="job-metadata">
                    <h4>Metadata</h4>
                    <pre>${escapeHtml(JSON.stringify(job.metadata, null, 2))}</pre>
                </div>` : ''}

                <div class="job-actions-panel">
                    ${isJobCancellable(job.status) ? `<button class="btn-danger" data-action="cancel" data-job-id="${escapeHtml(job.id)}">Cancel Job</button>` : ''}
                    ${isJobComplete(job.status) ? `<button class="btn-primary" data-action="result" data-job-id="${escapeHtml(job.id)}">View Results</button>` : ''}
                </div>

                <div id="job-result-container"></div>
            </div>
        `;

        container.innerHTML = detailsHtml;

        // Attach event listeners for action buttons
        container.querySelectorAll('button[data-action]').forEach(btn => {
            btn.addEventListener('click', () => {
                const jobId = btn.dataset.jobId;
                if (btn.dataset.action === 'cancel') {
                    cancelJob(jobId);
                } else if (btn.dataset.action === 'result') {
                    viewJobResult(jobId);
                }
            });
        });

        // If job is complete, automatically load results
        if (isJobComplete(job.status)) {
            viewJobResult(job.id);
        }
    } catch (error) {
        showError(container, error.message);
    }
}

function isJobComplete(status) {
    return ['completed', 'succeeded'].includes(status.toLowerCase());
}

async function viewJobResult(jobId) {
    const container = document.getElementById('job-result-container');
    if (!container) return;

    try {
        container.innerHTML = '<p class="placeholder">Loading results...</p>';

        const result = await api.getJobResult(jobId);

        const resultHtml = `
            <div class="result-panel">
                <h4>Execution Results</h4>

                <div class="result-stats">
                    <span><strong>Total Shots:</strong> ${escapeHtml(String(result.statistics.total_shots))}</span>
                    <span><strong>Unique Outcomes:</strong> ${escapeHtml(String(result.statistics.unique_outcomes))}</span>
                    ${result.execution_time_ms ? `<span><strong>Execution Time:</strong> ${escapeHtml(String(result.execution_time_ms))}ms</span>` : ''}
                    <span><strong>Most Frequent:</strong> ${escapeHtml(String(result.statistics.most_frequent))} (${escapeHtml(String(result.statistics.most_frequent_count))} times)</span>
                </div>

                <div id="histogram-container" class="histogram-container"></div>

                <div class="result-table-container">
                    <h5>Measurement Counts</h5>
                    <table class="result-table">
                        <thead>
                            <tr>
                                <th>Bitstring</th>
                                <th>Count</th>
                                <th>Probability</th>
                            </tr>
                        </thead>
                        <tbody>
                            ${result.bars.slice(0, 20).map(bar => `
                                <tr>
                                    <td class="mono">${escapeHtml(bar.bitstring)}</td>
                                    <td>${bar.count}</td>
                                    <td>${(bar.probability * 100).toFixed(2)}%</td>
                                </tr>
                            `).join('')}
                            ${result.bars.length > 20 ? `<tr><td colspan="3" class="more-results">...and ${result.bars.length - 20} more results</td></tr>` : ''}
                        </tbody>
                    </table>
                </div>
            </div>
        `;

        container.innerHTML = resultHtml;

        // Render histogram with D3
        renderHistogram(document.getElementById('histogram-container'), result.bars);
    } catch (error) {
        container.innerHTML = `<div class="error-message">${escapeHtml(error.message)}</div>`;
    }
}

function renderHistogram(container, bars) {
    if (!container || bars.length === 0) return;

    // Take top 15 for visualization
    const data = bars.slice(0, 15);

    const margin = { top: 20, right: 20, bottom: 60, left: 60 };
    const width = Math.min(container.clientWidth || 600, 800) - margin.left - margin.right;
    const height = 250 - margin.top - margin.bottom;

    const svg = d3.select(container)
        .append('svg')
        .attr('width', width + margin.left + margin.right)
        .attr('height', height + margin.top + margin.bottom)
        .append('g')
        .attr('transform', `translate(${margin.left},${margin.top})`);

    // X scale
    const x = d3.scaleBand()
        .domain(data.map(d => d.bitstring))
        .range([0, width])
        .padding(0.2);

    // Y scale
    const y = d3.scaleLinear()
        .domain([0, d3.max(data, d => d.probability)])
        .nice()
        .range([height, 0]);

    // Bars
    svg.selectAll('.bar')
        .data(data)
        .enter()
        .append('rect')
        .attr('class', 'histogram-bar')
        .attr('x', d => x(d.bitstring))
        .attr('y', d => y(d.probability))
        .attr('width', x.bandwidth())
        .attr('height', d => height - y(d.probability))
        .attr('fill', '#4fc3f7');

    // X axis
    svg.append('g')
        .attr('class', 'axis')
        .attr('transform', `translate(0,${height})`)
        .call(d3.axisBottom(x))
        .selectAll('text')
        .attr('transform', 'rotate(-45)')
        .style('text-anchor', 'end')
        .attr('dx', '-0.5em')
        .attr('dy', '0.5em');

    // Y axis
    svg.append('g')
        .attr('class', 'axis')
        .call(d3.axisLeft(y).ticks(5).tickFormat(d => `${(d * 100).toFixed(0)}%`));

    // Y axis label
    svg.append('text')
        .attr('class', 'axis-label')
        .attr('transform', 'rotate(-90)')
        .attr('y', 0 - margin.left)
        .attr('x', 0 - (height / 2))
        .attr('dy', '1em')
        .style('text-anchor', 'middle')
        .text('Probability');
}

// ============================================================================
// VQE View Controller
// ============================================================================

async function loadVqe() {
    const container = document.getElementById('vqe-chart-container');
    const legend = document.getElementById('vqe-legend');
    const info = document.getElementById('vqe-info');

    try {
        container.innerHTML = '<p class="placeholder">Loading VQE results...</p>';
        legend.innerHTML = '';
        info.innerHTML = '';

        const data = await api.getVqeDemo();

        // Render the convergence chart
        renderVQEChart(container, data);

        // Render legend
        legend.innerHTML = `
            <span class="vqe-legend-item">
                <span class="vqe-legend-swatch" style="background: var(--accent);"></span>
                VQE Energy
            </span>
            <span class="vqe-legend-item">
                <span class="vqe-legend-swatch vqe-legend-dashed" style="background: var(--success);"></span>
                Exact: ${data.exact_energy.toFixed(4)} Ha
            </span>
        `;

        // Render info panel
        const finalEnergy = data.final_energy;
        const error = data.error;
        const converged = error < 1e-4;

        info.innerHTML = `
            <div class="detail-item">
                <span class="label">Bond Distance</span>
                <span class="value">${data.bond_distance} &#x212B;</span>
            </div>
            <div class="detail-item">
                <span class="label">Final Energy</span>
                <span class="value">${finalEnergy.toFixed(7)} Ha</span>
            </div>
            <div class="detail-item">
                <span class="label">Exact Energy</span>
                <span class="value">${data.exact_energy.toFixed(7)} Ha</span>
            </div>
            <div class="detail-item">
                <span class="label">Error</span>
                <span class="value">${error.toExponential(2)} Ha</span>
            </div>
            <div class="detail-item">
                <span class="label">Iterations</span>
                <span class="value">${data.iterations.length}</span>
            </div>
            <div class="detail-item">
                <span class="label">Total Shots</span>
                <span class="value">${data.total_shots.toLocaleString()}</span>
            </div>
            <div class="detail-item">
                <span class="label">Backend</span>
                <span class="value">${escapeHtml(String(data.backend))}</span>
            </div>
            <div class="detail-item">
                <span class="label">Converged</span>
                <span class="value" style="color: ${converged ? 'var(--success)' : 'var(--warning)'};">${converged ? 'Yes' : 'No'}</span>
            </div>
        `;
    } catch (error) {
        showError(container, error.message);
    }
}

function renderVQEChart(container, data) {
    if (!container || !data.iterations || data.iterations.length === 0) return;

    container.innerHTML = '';

    const iterations = data.iterations;
    const exactEnergy = data.exact_energy;

    const margin = { top: 20, right: 30, bottom: 50, left: 70 };
    const width = Math.min(container.clientWidth || 700, 900) - margin.left - margin.right;
    const height = 350 - margin.top - margin.bottom;

    const svg = d3.select(container)
        .append('svg')
        .attr('width', width + margin.left + margin.right)
        .attr('height', height + margin.top + margin.bottom)
        .append('g')
        .attr('transform', `translate(${margin.left},${margin.top})`);

    // X scale — iteration number
    const x = d3.scaleLinear()
        .domain([0, d3.max(iterations, d => d.iteration)])
        .range([0, width]);

    // Y scale — energy
    const energies = iterations.map(d => d.energy);
    const yMin = Math.min(d3.min(energies), exactEnergy) - 0.02;
    const yMax = d3.max(energies) + 0.02;
    const y = d3.scaleLinear()
        .domain([yMin, yMax])
        .range([height, 0]);

    // Exact energy reference line (drawn first so it's behind)
    svg.append('line')
        .attr('class', 'vqe-exact')
        .attr('x1', 0)
        .attr('y1', y(exactEnergy))
        .attr('x2', width)
        .attr('y2', y(exactEnergy));

    // Line generator
    const line = d3.line()
        .x(d => x(d.iteration))
        .y(d => y(d.energy));

    // VQE energy path
    svg.append('path')
        .datum(iterations)
        .attr('class', 'vqe-line')
        .attr('fill', 'none')
        .attr('d', line);

    // Dots
    svg.selectAll('.vqe-dot')
        .data(iterations)
        .enter()
        .append('circle')
        .attr('class', 'vqe-dot')
        .attr('cx', d => x(d.iteration))
        .attr('cy', d => y(d.energy))
        .attr('r', 3.5);

    // X axis
    svg.append('g')
        .attr('class', 'axis')
        .attr('transform', `translate(0,${height})`)
        .call(d3.axisBottom(x).ticks(Math.min(iterations.length, 12)).tickFormat(d3.format('d')));

    // X axis label
    svg.append('text')
        .attr('class', 'axis-label')
        .attr('x', width / 2)
        .attr('y', height + margin.bottom - 8)
        .style('text-anchor', 'middle')
        .text('Iteration');

    // Y axis
    svg.append('g')
        .attr('class', 'axis')
        .call(d3.axisLeft(y).ticks(8).tickFormat(d => d.toFixed(2)));

    // Y axis label
    svg.append('text')
        .attr('class', 'axis-label')
        .attr('transform', 'rotate(-90)')
        .attr('y', -margin.left + 15)
        .attr('x', -(height / 2))
        .style('text-anchor', 'middle')
        .text('Energy (Ha)');
}

async function cancelJob(jobId) {
    if (!confirm('Are you sure you want to cancel this job?')) {
        return;
    }

    try {
        await api.deleteJob(jobId);
        loadJobs();
    } catch (error) {
        alert('Failed to cancel job: ' + error.message);
    }
}

// Utility functions
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function formatTime(isoString) {
    if (!isoString) return '-';
    const date = new Date(isoString);
    return date.toLocaleString();
}

function clearCircuit() {
    document.getElementById('qasm-input').value = '';
    document.getElementById('circuit-info').innerHTML = '';
    document.getElementById('circuit-container').innerHTML =
        '<p class="placeholder">Enter QASM3 code and click "Visualize" to see the circuit diagram.</p>';
    document.getElementById('compile-results').style.display = 'none';
    document.getElementById('target-select').value = '';
    state.circuit = null;
    state.compileResult = null;
}

// ============================================================================
// Evaluator View Controller
// ============================================================================

let evalReport = null;

async function runEvaluation() {
    const container = document.getElementById('eval-results');
    const exportBtn = document.getElementById('eval-export-btn');

    const qasm = document.getElementById('eval-qasm').value;
    if (!qasm.trim()) {
        showError(container, 'Please enter QASM3 code');
        return;
    }

    const target = document.getElementById('eval-target').value;
    const optLevel = parseInt(document.getElementById('eval-opt-level').value, 10);
    const orchestration = document.getElementById('eval-orchestration').checked;
    const emitter = document.getElementById('eval-emitter').checked;
    const schedulerSite = document.getElementById('eval-scheduler').value || null;

    const params = {
        qasm,
        target,
        optimization_level: optLevel,
        target_qubits: target === 'iqm' ? 20 : target === 'ibm' ? 20 : 10,
        orchestration,
        scheduler_site: orchestration ? schedulerSite : null,
        emit_target: emitter ? target : null,
    };

    try {
        container.innerHTML = '<p class="placeholder">Running evaluation pipeline...</p>';
        exportBtn.style.display = 'none';

        const result = await api.evaluate(params);
        evalReport = result;
        exportBtn.style.display = '';

        renderEvalReport(container, result);
    } catch (error) {
        showError(container, error.message);
    }
}

function renderEvalReport(container, r) {
    let html = '';

    // --- Summary Cards ---
    html += '<div class="eval-summary-grid">';
    html += evalCard('Qubits', r.input.num_qubits);
    html += evalCard('Input Depth', r.input.depth);
    html += evalCard('Input Gates', r.input.total_ops);
    html += evalCard('Compiled Depth', r.compilation.compiled_depth,
        r.compilation.depth_delta < 0 ? 'improved' : r.compilation.depth_delta > 0 ? 'degraded' : '',
        `${r.compilation.depth_delta >= 0 ? '+' : ''}${r.compilation.depth_delta}`);
    html += evalCard('Compiled Gates', r.compilation.compiled_ops,
        r.compilation.ops_delta < 0 ? 'improved' : r.compilation.ops_delta > 0 ? 'degraded' : '',
        `${r.compilation.ops_delta >= 0 ? '+' : ''}${r.compilation.ops_delta}`);
    html += evalCard('Passes', r.compilation.num_passes);
    const evalTimeStr = r.compilation.compile_time_us < 1000
        ? `${r.compilation.compile_time_us}\u00B5s`
        : `${(r.compilation.compile_time_us / 1000).toFixed(2)}ms`;
    const evalThroughputStr = r.compilation.throughput_gates_per_sec >= 1000000
        ? `${(r.compilation.throughput_gates_per_sec / 1000000).toFixed(1)}M gates/s`
        : `${(r.compilation.throughput_gates_per_sec / 1000).toFixed(0)}K gates/s`;
    html += evalCard('Compile Time', evalTimeStr, 'improved');
    html += evalCard('Throughput', evalThroughputStr, 'improved');
    html += '</div>';

    // --- Emitter Compliance ---
    if (r.emitter) {
        html += '<div class="eval-section">';
        html += `<h3>Emitter Compliance: ${escapeHtml(r.emitter.target)}</h3>`;

        // Coverage bar
        html += '<div class="eval-coverage-grid">';
        html += evalMetric('Native Coverage', `${(r.emitter.native_coverage * 100).toFixed(0)}%`);
        html += evalMetric('Materializable', `${(r.emitter.materializable_coverage * 100).toFixed(0)}%`);
        html += evalMetric('Expansion', `${r.emitter.estimated_expansion.toFixed(1)}x`);
        html += evalMetric('Emission', r.emitter.emission_success ? 'OK' : 'FAILED',
            r.emitter.emission_success ? 'improved' : 'degraded');
        html += evalMetric('Materializable', r.emitter.fully_materializable ? 'YES' : 'NO',
            r.emitter.fully_materializable ? 'improved' : 'degraded');
        html += '</div>';

        // Donut chart placeholder
        html += '<div id="eval-emitter-chart" class="eval-chart"></div>';

        // Gate materialization table
        if (r.emitter.gates.length > 0) {
            html += '<table class="eval-table"><thead><tr><th>Gate</th><th>Count</th><th>Status</th><th>Cost</th></tr></thead><tbody>';
            r.emitter.gates.forEach(g => {
                const statusClass = g.status === 'Native' ? 'tag-safe' : g.status === 'Decomposed' ? 'tag-conditional' : 'tag-violating';
                html += `<tr><td class="mono">${escapeHtml(g.gate)}</td><td>${escapeHtml(String(g.count))}</td><td><span class="${statusClass}">${escapeHtml(g.status)}</span></td><td>${g.cost !== null ? escapeHtml(String(g.cost)) : '-'}</td></tr>`;
            });
            html += '</tbody></table>';
        }

        // Losses
        if (r.emitter.losses.length > 0) {
            html += '<h4>Loss Documentation</h4>';
            html += '<table class="eval-table"><thead><tr><th>Capability</th><th>Category</th><th>Impact</th></tr></thead><tbody>';
            r.emitter.losses.forEach(l => {
                html += `<tr><td class="mono">${escapeHtml(l.capability)}</td><td>${escapeHtml(l.category)}</td><td>${escapeHtml(l.impact)}</td></tr>`;
            });
            html += '</tbody></table>';
        }
        html += '</div>';
    }

    // --- Orchestration ---
    if (r.orchestration) {
        html += '<div class="eval-section">';
        html += '<h3>Orchestration</h3>';
        html += '<div class="eval-coverage-grid">';
        html += evalMetric('Quantum Phases', r.orchestration.quantum_phases);
        html += evalMetric('Classical Phases', r.orchestration.classical_phases);
        html += evalMetric('Critical Path Cost', r.orchestration.critical_path_cost.toFixed(1));
        html += evalMetric('Max Parallel', r.orchestration.max_parallel_quantum);
        html += evalMetric('Parallelism', r.orchestration.parallelism_ratio.toFixed(2));
        html += evalMetric('Purely Quantum', r.orchestration.is_purely_quantum ? 'Yes' : 'No');
        html += '</div>';

        // Hybrid DAG
        html += '<div id="eval-hybrid-dag" class="eval-chart"></div>';
        html += '</div>';
    }

    // --- Scheduler ---
    if (r.scheduler) {
        html += '<div class="eval-section">';
        html += `<h3>Scheduler Fitness: ${escapeHtml(r.scheduler.site)} (${escapeHtml(r.scheduler.partition)})</h3>`;
        html += '<div class="eval-coverage-grid">';
        html += evalMetric('Qubits Fit', r.scheduler.qubits_fit ? 'YES' : 'NO',
            r.scheduler.qubits_fit ? 'improved' : 'degraded');
        html += evalMetric('Walltime Fit', r.scheduler.fits_walltime ? 'YES' : 'NO',
            r.scheduler.fits_walltime ? 'improved' : 'degraded');
        html += evalMetric('Fitness Score', r.scheduler.fitness_score.toFixed(2));
        html += evalMetric('Walltime (rec)', `${r.scheduler.recommended_walltime}s`);
        html += evalMetric('Batch Capacity', r.scheduler.batch_capacity);
        html += '</div>';
        html += `<div class="eval-assessment">${escapeHtml(r.scheduler.assessment)}</div>`;
        html += '</div>';
    }

    // --- Benchmark ---
    if (r.benchmark) {
        html += '<div class="eval-section">';
        html += '<h3>Benchmark (non-normative)</h3>';
        html += '<div class="eval-coverage-grid">';
        html += evalMetric('Suite', r.benchmark.name);
        html += evalMetric('Qubits', r.benchmark.num_qubits);
        html += evalMetric('Expected Gates', r.benchmark.expected_gates);
        html += '</div>';
        html += '</div>';
    }

    container.innerHTML = html;

    // Render D3 charts after DOM is updated
    if (r.emitter) {
        renderEmitterDonut(document.getElementById('eval-emitter-chart'), r.emitter);
    }
    if (r.orchestration && r.orchestration.nodes.length > 0) {
        renderHybridDag(document.getElementById('eval-hybrid-dag'), r.orchestration);
    }
}

function evalCard(label, value, cls, delta) {
    return `<div class="eval-card">
        <div class="eval-card-value ${cls || ''}">${escapeHtml(String(value))}${delta ? ` <small>(${escapeHtml(String(delta))})</small>` : ''}</div>
        <div class="eval-card-label">${escapeHtml(String(label))}</div>
    </div>`;
}

function evalMetric(label, value, cls) {
    return `<div class="eval-metric">
        <span class="eval-metric-value ${cls || ''}">${escapeHtml(String(value))}</span>
        <span class="eval-metric-label">${escapeHtml(String(label))}</span>
    </div>`;
}

function renderEmitterDonut(container, emitter) {
    if (!container) return;

    const data = [
        { label: 'Native', value: 0, color: '#00ff88' },
        { label: 'Decomposed', value: 0, color: '#ffaa00' },
        { label: 'Lost', value: 0, color: '#ff4444' },
    ];

    emitter.gates.forEach(g => {
        if (g.status === 'Native') data[0].value += g.count;
        else if (g.status === 'Decomposed') data[1].value += g.count;
        else data[2].value += g.count;
    });

    const filtered = data.filter(d => d.value > 0);
    if (filtered.length === 0) return;

    const width = 220, height = 220, radius = 90;
    const svg = d3.select(container)
        .append('svg')
        .attr('width', width + 160)
        .attr('height', height)
        .append('g')
        .attr('transform', `translate(${width / 2}, ${height / 2})`);

    const pie = d3.pie().value(d => d.value).sort(null);
    const arc = d3.arc().innerRadius(50).outerRadius(radius);

    svg.selectAll('path')
        .data(pie(filtered))
        .enter()
        .append('path')
        .attr('d', arc)
        .attr('fill', d => d.data.color)
        .attr('stroke', '#1a1a2e')
        .attr('stroke-width', 2);

    // Legend
    const legend = svg.append('g').attr('transform', `translate(${radius + 20}, ${-filtered.length * 12})`);
    filtered.forEach((d, i) => {
        const g = legend.append('g').attr('transform', `translate(0, ${i * 24})`);
        g.append('rect').attr('width', 14).attr('height', 14).attr('fill', d.color).attr('rx', 2);
        g.append('text').attr('x', 20).attr('y', 12).attr('fill', '#e8e8e8').attr('font-size', '12px')
            .text(`${d.label}: ${d.value}`);
    });
}

function renderHybridDag(container, orch) {
    if (!container || !orch.nodes || orch.nodes.length === 0) return;

    const nodes = orch.nodes;
    const edges = orch.edges;

    const nodeW = 100, nodeH = 50, gapX = 60, gapY = 0;
    const margin = { top: 20, right: 20, bottom: 20, left: 20 };
    const width = margin.left + margin.right + nodes.length * (nodeW + gapX);
    const height = margin.top + margin.bottom + nodeH + 40;

    const svg = d3.select(container)
        .append('svg')
        .attr('width', width)
        .attr('height', height)
        .append('g')
        .attr('transform', `translate(${margin.left}, ${margin.top})`);

    // Positions: lay out horizontally
    const pos = {};
    nodes.forEach((n, i) => {
        pos[n.index] = { x: i * (nodeW + gapX) + nodeW / 2, y: nodeH / 2 + 10 };
    });

    // Draw edges
    edges.forEach(e => {
        if (pos[e.from] && pos[e.to]) {
            svg.append('line')
                .attr('x1', pos[e.from].x + nodeW / 2 - 10)
                .attr('y1', pos[e.from].y)
                .attr('x2', pos[e.to].x - nodeW / 2 + 10)
                .attr('y2', pos[e.to].y)
                .attr('stroke', '#00d9ff')
                .attr('stroke-width', 2)
                .attr('marker-end', 'url(#arrowhead)');
        }
    });

    // Arrowhead marker
    svg.append('defs').append('marker')
        .attr('id', 'arrowhead')
        .attr('viewBox', '0 0 10 10')
        .attr('refX', 9).attr('refY', 5)
        .attr('markerWidth', 8).attr('markerHeight', 8)
        .attr('orient', 'auto')
        .append('path').attr('d', 'M 0 0 L 10 5 L 0 10 z').attr('fill', '#00d9ff');

    // Draw nodes
    nodes.forEach(n => {
        const p = pos[n.index];
        const color = n.kind === 'quantum' ? '#00d9ff' : '#ffaa00';

        svg.append('rect')
            .attr('x', p.x - nodeW / 2)
            .attr('y', p.y - nodeH / 2)
            .attr('width', nodeW)
            .attr('height', nodeH)
            .attr('rx', 6)
            .attr('fill', '#16213e')
            .attr('stroke', color)
            .attr('stroke-width', 2);

        svg.append('text')
            .attr('x', p.x)
            .attr('y', p.y - 5)
            .attr('text-anchor', 'middle')
            .attr('fill', color)
            .attr('font-size', '12px')
            .attr('font-weight', 'bold')
            .text(n.label);

        svg.append('text')
            .attr('x', p.x)
            .attr('y', p.y + 12)
            .attr('text-anchor', 'middle')
            .attr('fill', '#a0a0a0')
            .attr('font-size', '10px')
            .text(n.kind === 'quantum' ? `d:${n.depth || 0} g:${n.gate_count || 0}` : 'readout');
    });
}

function exportEvalJson() {
    if (!evalReport) return;
    const blob = new Blob([JSON.stringify(evalReport, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'eval-report.json';
    a.click();
    URL.revokeObjectURL(url);
}

// ============================================================================
// Initialization
// ============================================================================

document.addEventListener('DOMContentLoaded', async () => {
    // Load version from health endpoint
    try {
        const health = await api.health();
        document.getElementById('version').textContent = health.version;
    } catch (e) {
        console.error('Failed to load health info:', e);
    }

    // Set up navigation
    document.querySelectorAll('nav a').forEach(a => {
        a.addEventListener('click', e => {
            e.preventDefault();
            showView(a.dataset.view);
        });
    });

    // Set up circuit controls
    document.getElementById('visualize-btn').addEventListener('click', visualizeCircuit);
    document.getElementById('compile-btn').addEventListener('click', compileCircuit);
    document.getElementById('clear-btn').addEventListener('click', clearCircuit);
    document.getElementById('refresh-backends-btn').addEventListener('click', loadBackends);
    document.getElementById('refresh-jobs-btn').addEventListener('click', loadJobs);
    document.getElementById('eval-run-btn').addEventListener('click', runEvaluation);
    document.getElementById('eval-export-btn').addEventListener('click', exportEvalJson);

    // Allow Ctrl+Enter to visualize
    document.getElementById('qasm-input').addEventListener('keydown', e => {
        if (e.ctrlKey && e.key === 'Enter') {
            visualizeCircuit();
        }
    });

    // Handle hash-based routing
    const validViews = ['circuits', 'backends', 'jobs', 'eval', 'vqe', 'nathan'];
    const hashView = location.hash.replace('#', '');
    showView(validViews.includes(hashView) ? hashView : 'circuits');

    window.addEventListener('hashchange', () => {
        const view = location.hash.replace('#', '');
        if (validViews.includes(view) && view !== state.currentView) {
            showView(view);
        }
    });
});

// =====================================================================
// Nathan — Research Optimizer
// =====================================================================

const NATHAN_API = '/api/nathan';
let nathanInitialized = false;

function initNathan() {
    if (nathanInitialized) return;
    nathanInitialized = true;

    document.getElementById('nathan-analyze-btn').addEventListener('click', nathanAnalyze);

    document.getElementById('nathan-template-btn').addEventListener('click', () => {
        const picker = document.getElementById('nathan-template-picker');
        picker.style.display = picker.style.display === 'none' ? 'block' : 'none';
    });

    document.getElementById('nathan-load-template-btn').addEventListener('click', nathanLoadTemplate);

    document.getElementById('nathan-chat-send').addEventListener('click', nathanSendChat);
    document.getElementById('nathan-chat-input').addEventListener('keydown', (e) => {
        if (e.key === 'Enter') nathanSendChat();
    });
}

async function nathanAnalyze() {
    const code = document.getElementById('nathan-code').value.trim();
    const lang = document.getElementById('nathan-lang').value;
    const backend = document.getElementById('nathan-backend').value || null;
    const results = document.getElementById('nathan-results');
    const btn = document.getElementById('nathan-analyze-btn');

    if (!code) {
        results.innerHTML = '<p class="placeholder">Please enter circuit code.</p>';
        return;
    }

    btn.disabled = true;
    btn.textContent = 'Analyzing...';
    results.innerHTML = '<p class="placeholder">Analyzing circuit...</p>';

    try {
        const resp = await fetch(`${NATHAN_API}/analyze`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ code, language: lang, backend_id: backend }),
        });

        if (resp.status === 429) {
            results.innerHTML = '<p class="placeholder" style="color:var(--warning);">Rate limit exceeded. Please wait a moment.</p>';
            return;
        }

        if (!resp.ok) {
            const err = await resp.json().catch(() => ({}));
            results.innerHTML = `<p class="placeholder" style="color:#ef4444;">Error: ${err.detail || resp.status}</p>`;
            return;
        }

        const data = await resp.json();
        renderNathanResults(data);

        // Show chat section after first analysis
        document.getElementById('nathan-chat-section').style.display = '';
    } catch (e) {
        results.innerHTML = '<p class="placeholder" style="color:#ef4444;">Failed to connect to Nathan API.</p>';
    } finally {
        btn.disabled = false;
        btn.textContent = 'Analyze';
    }
}

function renderNathanResults(data) {
    const results = document.getElementById('nathan-results');
    let html = '';

    // Circuit stats
    if (data.circuit) {
        const c = data.circuit;
        html += `<div class="nathan-stats-grid">
            <div class="nathan-stat"><span class="nathan-stat-label">Qubits</span><span class="nathan-stat-value">${c.num_qubits}</span></div>
            <div class="nathan-stat"><span class="nathan-stat-label">Gates</span><span class="nathan-stat-value">${c.total_gates}</span></div>
            <div class="nathan-stat"><span class="nathan-stat-label">Depth</span><span class="nathan-stat-value">${c.depth}</span></div>
            <div class="nathan-stat"><span class="nathan-stat-label">Pattern</span><span class="nathan-stat-value">${c.detected_pattern}</span></div>
        </div>`;
    }

    // Classification
    const suitPct = Math.round((data.suitability || 0) * 100);
    const suitColor = suitPct >= 60 ? 'var(--accent)' : suitPct >= 35 ? '#eab308' : '#ef4444';
    html += `<div class="nathan-classification">
        <div><strong>Problem:</strong> ${data.problem_type || 'unknown'}</div>
        <div><strong>Algorithm:</strong> ${data.recommended_algorithm || 'N/A'}</div>
        <div><strong>Suitability:</strong> <span style="color:${suitColor};font-weight:600;">${suitPct}%</span></div>
        <div><strong>Est. Qubits:</strong> ${data.estimated_qubits || 'N/A'}</div>
    </div>`;

    // Papers
    if (data.papers && data.papers.length > 0) {
        html += '<h3>Relevant Papers</h3><div class="nathan-papers">';
        for (const p of data.papers) {
            html += `<div class="nathan-paper">
                <a href="${escapeHtml(p.arxiv_url)}" target="_blank" rel="noopener">${escapeHtml(p.title)}</a>
                ${p.relevance ? `<span class="nathan-paper-meta">${escapeHtml(p.relevance)}</span>` : ''}
            </div>`;
        }
        html += '</div>';
    }

    // Suggestions
    if (data.suggestions && data.suggestions.length > 0) {
        html += '<h3>Suggestions</h3>';
        for (const s of data.suggestions) {
            html += `<div class="nathan-suggestion">
                <div class="nathan-suggestion-header">
                    <span>${escapeHtml(s.title)}</span>
                    ${s.impact ? `<span class="nathan-impact nathan-impact-${s.impact}">${s.impact}</span>` : ''}
                </div>
                <p>${escapeHtml(s.description)}</p>
                ${s.qasm3 ? `<pre class="nathan-code">${escapeHtml(s.qasm3)}</pre>` : ''}
            </div>`;
        }
    }

    // LLM response
    if (data.raw_llm_response || data.summary) {
        html += `<h3>Analysis</h3><div class="nathan-llm-response">${escapeHtml(data.raw_llm_response || data.summary)}</div>`;
    }

    results.innerHTML = html;
}

function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str || '';
    return div.innerHTML;
}

async function nathanLoadTemplate() {
    const templateId = document.getElementById('nathan-template-select').value;
    try {
        const resp = await fetch(`${NATHAN_API}/templates?problem_type=${templateId}`);
        if (!resp.ok) return;
        const data = await resp.json();
        if (data.qasm3) {
            document.getElementById('nathan-code').value = data.qasm3;
            document.getElementById('nathan-lang').value = 'qasm3';
            document.getElementById('nathan-template-picker').style.display = 'none';
        }
    } catch { /* silently fail */ }
}

async function nathanSendChat() {
    const input = document.getElementById('nathan-chat-input');
    const msg = input.value.trim();
    if (!msg) return;

    input.value = '';
    const container = document.getElementById('nathan-chat-messages');

    // User message
    container.innerHTML += `<div class="nathan-msg nathan-msg-user"><strong>You:</strong> ${escapeHtml(msg)}</div>`;

    try {
        const resp = await fetch(`${NATHAN_API}/chat`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ message: msg, context: '' }),
        });
        const data = await resp.json();
        container.innerHTML += `<div class="nathan-msg nathan-msg-assistant"><strong>Nathan:</strong> ${escapeHtml(data.message)}</div>`;
    } catch {
        container.innerHTML += `<div class="nathan-msg nathan-msg-assistant" style="color:#ef4444;"><strong>Nathan:</strong> Could not reach API.</div>`;
    }

    container.scrollTop = container.scrollHeight;
}
