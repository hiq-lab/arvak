"""arvak-bquant — Arvak quantum computing client for portfolio optimization.

Pure-Python package with no Rust/PyO3 dependency.  Designed for use in
Bloomberg BQuant and similar managed notebook environments.

Example
-------
>>> import arvak_bquant as aq
>>> spec = aq.PortfolioSpec(
...     expected_returns=mu,
...     covariance_matrix=sigma,
...     risk_aversion=0.5,
...     budget=3,
...     asset_names=["AAPL", "MSFT", "GOOG"],
... )
>>> qubo = aq.portfolio_to_qubo(spec)
>>> qasm = aq.qaoa_circuit_qasm3(qubo, p=2)
>>> client = aq.ArvakClient(api_key="...")
>>> result = client.run(qasm, backend_id="simulator", shots=4096)
>>> portfolio = aq.interpret_portfolio_result(result, spec)
"""

from .client import ArvakClient
from .exceptions import (
    ArvakAPIError,
    ArvakCompilationError,
    ArvakConnectionError,
    ArvakError,
    ArvakJobError,
    ArvakTimeoutError,
)
from .portfolio import PortfolioSpec, portfolio_to_qubo
from .qaoa import qaoa_circuit_qasm3
from .qubo import IsingProblem, QUBOProblem, qubo_to_ising
from .result import PortfolioResult, PortfolioSolution, interpret_portfolio_result
from .types import BackendInfo, CompileResult, JobResult, JobStatus

__all__ = [
    # Client
    "ArvakClient",
    # Portfolio
    "PortfolioSpec",
    "portfolio_to_qubo",
    # QUBO / Ising
    "QUBOProblem",
    "IsingProblem",
    "qubo_to_ising",
    # QAOA
    "qaoa_circuit_qasm3",
    # Results
    "interpret_portfolio_result",
    "PortfolioResult",
    "PortfolioSolution",
    # Types
    "BackendInfo",
    "CompileResult",
    "JobResult",
    "JobStatus",
    # Exceptions
    "ArvakError",
    "ArvakAPIError",
    "ArvakConnectionError",
    "ArvakTimeoutError",
    "ArvakCompilationError",
    "ArvakJobError",
]
