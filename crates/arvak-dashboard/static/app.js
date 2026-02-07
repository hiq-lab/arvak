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
// View Controllers
// ============================================================================

function showView(viewName) {
    state.currentView = viewName;

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
            <span><span class="label">Name:</span> <span class="value">${circuit.name}</span></span>
            <span><span class="label">Qubits:</span> <span class="value">${circuit.num_qubits}</span></span>
            <span><span class="label">Depth:</span> <span class="value">${circuit.depth}</span></span>
            <span><span class="label">Gates:</span> <span class="value">${circuit.num_ops}</span></span>
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
            <span><span class="label">Name:</span> <span class="value">${result.before.name}</span></span>
            <span><span class="label">Target:</span> <span class="value">${target.toUpperCase()}</span></span>
            <span><span class="label">Opt Level:</span> <span class="value">${optLevel}</span></span>
        `;

        // Render original circuit in main container
        circuitRenderer.render(container, result.before);

        // Show compilation results
        resultsPanel.style.display = 'block';

        // Update stats
        const stats = result.stats;
        const depthChange = stats.compiled_depth - stats.original_depth;
        const gateChange = stats.gates_after - stats.gates_before;

        document.getElementById('compile-stats').innerHTML = `
            <span><span class="label">Original Depth:</span> <span class="value">${stats.original_depth}</span></span>
            <span><span class="label">Compiled Depth:</span> <span class="value ${depthChange < 0 ? 'improved' : depthChange > 0 ? 'degraded' : ''}">${stats.compiled_depth} (${depthChange >= 0 ? '+' : ''}${depthChange})</span></span>
            <span><span class="label">Original Gates:</span> <span class="value">${stats.gates_before}</span></span>
            <span><span class="label">Compiled Gates:</span> <span class="value ${gateChange < 0 ? 'improved' : gateChange > 0 ? 'degraded' : ''}">${stats.gates_after} (${gateChange >= 0 ? '+' : ''}${gateChange})</span></span>
        `;

        // Render before/after circuits
        circuitRenderer.render(document.getElementById('circuit-before'), result.before);
        circuitRenderer.render(document.getElementById('circuit-after'), result.after);

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
            card.innerHTML = `
                <h3>
                    ${backend.name}
                    <span class="status ${backend.available ? 'available' : 'unavailable'}"></span>
                </h3>
                <div class="info">
                    <span><strong>Type:</strong> ${backend.is_simulator ? 'Simulator' : 'Hardware'}</span>
                    <span><strong>Qubits:</strong> ${backend.num_qubits}</span>
                    <span><strong>Status:</strong> ${backend.available ? 'Available' : 'Unavailable'}</span>
                </div>
                <div class="gates">
                    <strong>Native gates:</strong>
                    ${backend.native_gates.length > 0
                        ? backend.native_gates.map(g => `<span class="tag">${g}</span>`).join('')
                        : '<span class="tag">universal</span>'}
                </div>
            `;
            grid.appendChild(card);
        });

        container.innerHTML = '';
        container.appendChild(grid);
    } catch (error) {
        showError(container, error.message);
    }
}

function showError(container, message) {
    container.innerHTML = `<div class="error-message">${message}</div>`;
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
                <tr class="job-row" data-job-id="${job.id}">
                    <td class="job-id" title="${job.id}">${job.id.substring(0, 8)}...</td>
                    <td class="job-name">${escapeHtml(job.name)}</td>
                    <td>
                        <span class="status-badge status-${job.status.toLowerCase()}">${job.status}</span>
                        ${job.status_details ? `<span class="status-details" title="${escapeHtml(job.status_details)}">ℹ️</span>` : ''}
                    </td>
                    <td>${job.backend || '-'}</td>
                    <td>${job.shots}</td>
                    <td>${job.priority}</td>
                    <td class="job-time">${formatTime(job.created_at)}</td>
                    <td class="job-actions">
                        <button class="btn-small" onclick="viewJobDetails('${job.id}')">View</button>
                        ${isJobCancellable(job.status) ? `<button class="btn-small btn-danger" onclick="cancelJob('${job.id}')">Cancel</button>` : ''}
                    </td>
                </tr>
            `).join('')}
        </tbody>
    `;

    container.innerHTML = '';
    container.appendChild(table);
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
                    <span class="status-badge status-${job.status.toLowerCase()}">${job.status}</span>
                </div>

                <div class="job-details-grid">
                    <div class="detail-item">
                        <span class="label">Job ID</span>
                        <span class="value">${job.id}</span>
                    </div>
                    <div class="detail-item">
                        <span class="label">Status</span>
                        <span class="value">${job.status}${job.status_details ? ' - ' + escapeHtml(job.status_details) : ''}</span>
                    </div>
                    <div class="detail-item">
                        <span class="label">Backend</span>
                        <span class="value">${job.backend || 'Not assigned'}</span>
                    </div>
                    <div class="detail-item">
                        <span class="label">Shots</span>
                        <span class="value">${job.shots}</span>
                    </div>
                    <div class="detail-item">
                        <span class="label">Priority</span>
                        <span class="value">${job.priority}</span>
                    </div>
                    <div class="detail-item">
                        <span class="label">Circuits</span>
                        <span class="value">${job.num_circuits}</span>
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
                    <pre>${JSON.stringify(job.metadata, null, 2)}</pre>
                </div>` : ''}

                <div class="job-actions-panel">
                    ${isJobCancellable(job.status) ? `<button class="btn-danger" onclick="cancelJob('${job.id}')">Cancel Job</button>` : ''}
                    ${isJobComplete(job.status) ? `<button class="btn-primary" onclick="viewJobResult('${job.id}')">View Results</button>` : ''}
                </div>

                <div id="job-result-container"></div>
            </div>
        `;

        container.innerHTML = detailsHtml;

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
                    <span><strong>Total Shots:</strong> ${result.statistics.total_shots}</span>
                    <span><strong>Unique Outcomes:</strong> ${result.statistics.unique_outcomes}</span>
                    ${result.execution_time_ms ? `<span><strong>Execution Time:</strong> ${result.execution_time_ms}ms</span>` : ''}
                    <span><strong>Most Frequent:</strong> ${result.statistics.most_frequent} (${result.statistics.most_frequent_count} times)</span>
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
                                    <td class="mono">${bar.bitstring}</td>
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
        container.innerHTML = `<div class="error-message">${error.message}</div>`;
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

    // Allow Ctrl+Enter to visualize
    document.getElementById('qasm-input').addEventListener('keydown', e => {
        if (e.ctrlKey && e.key === 'Enter') {
            visualizeCircuit();
        }
    });

    // Show initial view
    showView('circuits');
});
