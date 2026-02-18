"""Rich display rendering for Nathan reports.

Provides HTML output for Jupyter notebooks via _repr_html_().
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .report import AnalysisReport, ChatResponse


def report_to_html(report: AnalysisReport) -> str:
    """Render an AnalysisReport as rich HTML for Jupyter."""
    suit_pct = int(report.suitability * 100)
    suit_color = "#22c55e" if suit_pct >= 60 else "#eab308" if suit_pct >= 35 else "#ef4444"

    # Circuit stats section
    circuit_html = ""
    if report.circuit:
        c = report.circuit
        circuit_html = f"""
        <div style="display:grid;grid-template-columns:repeat(4,1fr);gap:8px;margin-bottom:12px;">
            <div style="background:#1a1a24;padding:8px 12px;border-radius:6px;text-align:center;">
                <div style="font-size:11px;color:#606070;text-transform:uppercase;">Qubits</div>
                <div style="font-size:18px;font-weight:600;color:#f0f0f5;">{c.num_qubits}</div>
            </div>
            <div style="background:#1a1a24;padding:8px 12px;border-radius:6px;text-align:center;">
                <div style="font-size:11px;color:#606070;text-transform:uppercase;">Gates</div>
                <div style="font-size:18px;font-weight:600;color:#f0f0f5;">{c.total_gates}</div>
            </div>
            <div style="background:#1a1a24;padding:8px 12px;border-radius:6px;text-align:center;">
                <div style="font-size:11px;color:#606070;text-transform:uppercase;">Depth</div>
                <div style="font-size:18px;font-weight:600;color:#f0f0f5;">{c.depth}</div>
            </div>
            <div style="background:#1a1a24;padding:8px 12px;border-radius:6px;text-align:center;">
                <div style="font-size:11px;color:#606070;text-transform:uppercase;">Pattern</div>
                <div style="font-size:14px;font-weight:500;color:#f0f0f5;">{c.detected_pattern}</div>
            </div>
        </div>"""

    # Papers section
    papers_html = ""
    if report.papers:
        paper_items = ""
        for p in report.papers:
            paper_items += f"""
            <div style="padding:6px 0;border-bottom:1px solid #2a2a35;">
                <a href="{_esc(p.arxiv_url)}" target="_blank" rel="noopener"
                   style="color:#6366f1;text-decoration:none;font-size:13px;">{_esc(p.title)}</a>
                {f'<div style="font-size:11px;color:#606070;margin-top:2px;">{_esc(p.relevance)}</div>' if p.relevance else ''}
            </div>"""
        papers_html = f"""
        <div style="margin-top:12px;">
            <div style="font-size:12px;font-weight:600;color:#a0a0b0;text-transform:uppercase;
                        letter-spacing:0.05em;margin-bottom:6px;">Relevant Papers</div>
            {paper_items}
        </div>"""

    # Suggestions section
    suggestions_html = ""
    if report.suggestions:
        sugg_items = ""
        for s in report.suggestions:
            impact_color = {"high": "#22c55e", "medium": "#eab308", "low": "#a0a0b0"}.get(
                s.impact, "#a0a0b0"
            )
            code_block = ""
            if s.qasm3:
                code_block = f"""
                <pre style="background:#16161e;border:1px solid #2a2a35;border-radius:4px;
                            padding:8px;margin-top:6px;font-size:12px;overflow-x:auto;
                            font-family:'JetBrains Mono',monospace;color:#f0f0f5;
                            white-space:pre;">{_esc(s.qasm3)}</pre>"""
            sugg_items += f"""
            <div style="background:#1a1a24;border:1px solid #2a2a35;border-radius:6px;
                        padding:10px;margin-bottom:6px;">
                <div style="display:flex;justify-content:space-between;align-items:center;">
                    <span style="font-size:13px;font-weight:500;color:#f0f0f5;">{_esc(s.title)}</span>
                    {f'<span style="font-size:10px;padding:2px 8px;border-radius:10px;background:rgba(0,0,0,0.3);color:{impact_color};">{s.impact.upper()}</span>' if s.impact else ''}
                </div>
                <div style="font-size:12px;color:#a0a0b0;margin-top:4px;">{_esc(s.description)}</div>
                {code_block}
            </div>"""
        suggestions_html = f"""
        <div style="margin-top:12px;">
            <div style="font-size:12px;font-weight:600;color:#a0a0b0;text-transform:uppercase;
                        letter-spacing:0.05em;margin-bottom:6px;">Suggestions</div>
            {sugg_items}
        </div>"""

    # Summary section
    summary_html = ""
    if report.summary:
        summary_html = f"""
        <div style="margin-top:12px;font-size:13px;color:#f0f0f5;line-height:1.7;
                    border-top:1px solid #2a2a35;padding-top:12px;">
            {_markdown_to_html(report.summary)}
        </div>"""

    return f"""
    <div style="background:#12121a;border:1px solid #2a2a35;border-radius:8px;padding:16px;
                font-family:'DM Sans',-apple-system,BlinkMacSystemFont,sans-serif;max-width:700px;">
        <div style="display:flex;align-items:center;gap:10px;margin-bottom:12px;">
            <div style="width:28px;height:28px;background:linear-gradient(135deg,#6366f1,#8b5cf6);
                        border-radius:6px;display:flex;align-items:center;justify-content:center;
                        color:white;font-weight:700;font-size:13px;">N</div>
            <div>
                <span style="font-weight:600;color:#f0f0f5;">Nathan Analysis</span>
                <span style="color:#606070;font-size:12px;margin-left:8px;">{_esc(report.problem_type)}</span>
            </div>
        </div>

        {circuit_html}

        <div style="display:grid;grid-template-columns:repeat(3,1fr);gap:8px;margin-bottom:8px;">
            <div style="background:#1a1a24;padding:8px 12px;border-radius:6px;">
                <div style="font-size:11px;color:#606070;text-transform:uppercase;">Suitability</div>
                <div style="font-size:16px;font-weight:600;color:{suit_color};">{suit_pct}%</div>
                <div style="width:100%;height:4px;background:#0a0a0f;border-radius:2px;margin-top:4px;">
                    <div style="width:{suit_pct}%;height:100%;background:{suit_color};border-radius:2px;"></div>
                </div>
            </div>
            <div style="background:#1a1a24;padding:8px 12px;border-radius:6px;">
                <div style="font-size:11px;color:#606070;text-transform:uppercase;">Algorithm</div>
                <div style="font-size:14px;font-weight:500;color:#f0f0f5;">{_esc(report.recommended_algorithm or 'N/A')}</div>
            </div>
            <div style="background:#1a1a24;padding:8px 12px;border-radius:6px;">
                <div style="font-size:11px;color:#606070;text-transform:uppercase;">Est. Qubits</div>
                <div style="font-size:16px;font-weight:600;color:#f0f0f5;">{report.estimated_qubits}</div>
            </div>
        </div>

        {papers_html}
        {suggestions_html}
        {summary_html}
    </div>"""


def chat_to_html(response: ChatResponse) -> str:
    """Render a ChatResponse as rich HTML for Jupyter."""
    papers_html = ""
    if response.papers:
        items = "".join(
            f'<li><a href="{_esc(p.arxiv_url)}" target="_blank" rel="noopener" '
            f'style="color:#6366f1;text-decoration:none;">{_esc(p.title)}</a></li>'
            for p in response.papers
        )
        papers_html = f"""
        <div style="margin-top:10px;padding-top:10px;border-top:1px solid #2a2a35;">
            <div style="font-size:11px;color:#606070;text-transform:uppercase;margin-bottom:4px;">
                Referenced Papers</div>
            <ul style="margin:0;padding-left:16px;font-size:12px;">{items}</ul>
        </div>"""

    return f"""
    <div style="background:#12121a;border:1px solid #2a2a35;border-radius:8px;padding:16px;
                font-family:'DM Sans',-apple-system,BlinkMacSystemFont,sans-serif;max-width:700px;">
        <div style="display:flex;align-items:center;gap:8px;margin-bottom:10px;">
            <div style="width:24px;height:24px;background:linear-gradient(135deg,#6366f1,#8b5cf6);
                        border-radius:5px;display:flex;align-items:center;justify-content:center;
                        color:white;font-weight:700;font-size:11px;">N</div>
            <span style="font-weight:500;color:#a0a0b0;font-size:12px;">Nathan</span>
        </div>
        <div style="font-size:13px;color:#f0f0f5;line-height:1.7;">
            {_markdown_to_html(response.message)}
        </div>
        {papers_html}
    </div>"""


def _esc(text: str) -> str:
    """Escape HTML special characters."""
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def _markdown_to_html(text: str) -> str:
    """Minimal markdown to HTML conversion for display."""
    import re

    html = _esc(text)

    # Code blocks
    html = re.sub(
        r"```\w*\n(.*?)```",
        r'<pre style="background:#16161e;border:1px solid #2a2a35;border-radius:4px;'
        r"padding:8px;font-family:'JetBrains Mono',monospace;font-size:12px;"
        r'overflow-x:auto;color:#f0f0f5;white-space:pre;">\1</pre>',
        html,
        flags=re.DOTALL,
    )
    # Inline code
    html = re.sub(
        r"`([^`]+)`",
        r'<code style="background:#16161e;padding:1px 4px;border-radius:3px;'
        r"font-family:'JetBrains Mono',monospace;font-size:12px;\">\1</code>",
        html,
    )
    # Bold
    html = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", html)
    # Headers
    html = re.sub(r"^### (.+)$", r'<div style="font-weight:600;margin-top:8px;">\1</div>', html, flags=re.MULTILINE)
    html = re.sub(r"^## (.+)$", r'<div style="font-size:15px;font-weight:600;margin-top:10px;">\1</div>', html, flags=re.MULTILINE)
    # Lists
    html = re.sub(r"^- (.+)$", r'<div style="padding-left:12px;">&#8226; \1</div>', html, flags=re.MULTILINE)
    # Paragraphs
    html = html.replace("\n\n", "<br><br>")
    html = html.replace("\n", "<br>")

    return html
