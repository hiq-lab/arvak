"""Launch Arvak demo notebooks.

Usage from Python:
    >>> import arvak
    >>> arvak.demo()

Usage from terminal:
    $ arvak-demo
    $ arvak-demo --copy   # copy notebook to cwd without launching
"""

from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

_DEMO_NOTEBOOK = "qiskit_user_journey.ipynb"
_DEMOS_DIR = Path(__file__).parent / "_demos"


def _get_notebook_source() -> Path:
    """Locate the bundled demo notebook inside the package."""
    return _DEMOS_DIR / _DEMO_NOTEBOOK


def copy_notebook(dest_dir: str | Path | None = None) -> Path:
    """Copy the demo notebook to a destination directory.

    Parameters
    ----------
    dest_dir : str | Path | None
        Where to copy the notebook. Defaults to the current directory.

    Returns
    -------
    Path
        Path to the copied notebook.
    """
    dest_dir = Path(dest_dir or os.getcwd())
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest = dest_dir / _DEMO_NOTEBOOK

    src = _get_notebook_source()
    if not src.exists():
        raise FileNotFoundError(
            f"Demo notebook not found in package. "
            f"Expected at {src}. Try reinstalling: pip install arvak[qiskit]"
        )

    shutil.copy2(src, dest)
    return dest


def launch(copy_to: str | Path | None = None):
    """Copy the demo notebook to cwd and launch Jupyter.

    Parameters
    ----------
    copy_to : str | Path | None
        Directory to copy the notebook to. Defaults to current directory.
    """
    notebook_path = copy_notebook(copy_to)
    print(f"Demo notebook: {notebook_path}")

    # Check if Jupyter is available
    jupyter = shutil.which("jupyter")
    if jupyter is None:
        print(
            "\nJupyter not found. Install with:\n"
            "  pip install arvak[notebook]\n"
            f"\nThen open the notebook manually:\n"
            f"  jupyter notebook {notebook_path}"
        )
        return

    print("Launching Jupyter...")
    subprocess.run(
        [jupyter, "notebook", str(notebook_path)],
        check=False,
    )


def main():
    """CLI entry point for arvak-demo."""
    import argparse

    parser = argparse.ArgumentParser(
        prog="arvak-demo",
        description="Launch the Arvak + Qiskit demo notebook",
    )
    parser.add_argument(
        "--copy",
        action="store_true",
        help="Copy the notebook to the current directory without launching Jupyter",
    )
    parser.add_argument(
        "--dir",
        type=str,
        default=None,
        help="Directory to copy the notebook to (default: current directory)",
    )
    args = parser.parse_args()

    if args.copy:
        path = copy_notebook(args.dir)
        print(f"Notebook copied to: {path}")
    else:
        launch(args.dir)


if __name__ == "__main__":
    main()
