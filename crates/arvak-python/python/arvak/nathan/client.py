"""HTTP client for the Nathan API at arvak.io/api/nathan."""

from __future__ import annotations

import logging

import httpx

from .anonymize import anonymize_code
from .report import AnalysisReport, ChatResponse, CircuitStats, Paper, Suggestion

logger = logging.getLogger(__name__)


class NathanClient:
    """Synchronous HTTP client for the Nathan API.

    Uses httpx for HTTP requests. Designed for notebook/script use
    where async is not needed.
    """

    def __init__(self, api_url: str, api_key: str = ""):
        self._url = api_url.rstrip("/")
        self._api_key = api_key
        self._client = httpx.Client(timeout=60.0)

    def _headers(self) -> dict[str, str]:
        headers = {"Content-Type": "application/json"}
        if self._api_key:
            headers["Authorization"] = f"Bearer {self._api_key}"
        return headers

    def analyze(
        self,
        code: str,
        language: str = "qasm3",
        backend_id: str | None = None,
        anonymize: bool = True,
    ) -> AnalysisReport:
        """Call /analyze and return an AnalysisReport."""
        submitted_code = anonymize_code(code, language) if anonymize else code
        payload: dict = {"code": submitted_code, "language": language}
        if backend_id:
            payload["backend_id"] = backend_id

        try:
            resp = self._client.post(
                f"{self._url}/analyze",
                json=payload,
                headers=self._headers(),
            )
            resp.raise_for_status()
            data = resp.json()
        except httpx.HTTPStatusError as e:
            if e.response.status_code == 429:
                detail = e.response.json().get("detail", "Rate limit exceeded")
                raise RuntimeError(f"Nathan rate limit: {detail}") from e
            raise RuntimeError(f"Nathan API error: {e.response.status_code}") from e
        except httpx.ConnectError as e:
            raise RuntimeError(
                "Could not connect to Nathan API. "
                "Check your network or try arvak.nathan.configure(api_url=...)"
            ) from e

        return self._parse_report(data)

    def chat(self, message: str, context: str = "") -> ChatResponse:
        """Call /chat and return a ChatResponse."""
        try:
            resp = self._client.post(
                f"{self._url}/chat",
                json={"message": message, "context": context},
                headers=self._headers(),
            )
            resp.raise_for_status()
            data = resp.json()
        except httpx.HTTPStatusError as e:
            raise RuntimeError(f"Nathan API error: {e.response.status_code}") from e
        except httpx.ConnectError as e:
            raise RuntimeError("Could not connect to Nathan API.") from e

        papers = [
            Paper(
                title=p.get("title", ""),
                arxiv_url=p.get("arxiv_url", ""),
                relevance=p.get("relevance", ""),
            )
            for p in data.get("papers", [])
        ]

        return ChatResponse(message=data.get("message", ""), papers=papers)

    def _parse_report(self, data: dict) -> AnalysisReport:
        """Parse API JSON response into an AnalysisReport."""
        circuit_data = data.get("circuit")
        circuit = None
        if circuit_data:
            circuit = CircuitStats(
                num_qubits=circuit_data.get("num_qubits", 0),
                total_gates=circuit_data.get("total_gates", 0),
                gate_breakdown=circuit_data.get("gate_breakdown", ""),
                depth=circuit_data.get("depth", 0),
                detected_pattern=circuit_data.get("detected_pattern", "unknown"),
                language=circuit_data.get("language", "qasm3"),
            )

        papers = [
            Paper(
                title=p.get("title", ""),
                arxiv_url=p.get("arxiv_url", ""),
                problem_type=p.get("problem_type", ""),
                algorithm=p.get("algorithm", ""),
                relevance=p.get("relevance", ""),
            )
            for p in data.get("papers", [])
        ]

        suggestions = [
            Suggestion(
                title=s.get("title", ""),
                description=s.get("description", ""),
                qasm3=s.get("qasm3", ""),
                impact=s.get("impact", ""),
            )
            for s in data.get("suggestions", [])
        ]

        return AnalysisReport(
            summary=data.get("summary", data.get("raw_llm_response", "")),
            problem_type=data.get("problem_type", "unknown"),
            suitability=data.get("suitability", 0.0),
            recommended_algorithm=data.get("recommended_algorithm", ""),
            estimated_qubits=data.get("estimated_qubits", 0),
            circuit=circuit,
            papers=papers,
            suggestions=suggestions,
            hardware_fit=data.get("hardware_fit", ""),
            estimated_error_rate=data.get("estimated_error_rate", ""),
            recommended_shots=data.get("recommended_shots", 1024),
        )
