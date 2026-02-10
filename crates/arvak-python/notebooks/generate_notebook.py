#!/usr/bin/env python3
"""Generate a new framework integration notebook from template.

Usage:
    python generate_notebook.py <framework_name>

Example:
    python generate_notebook.py qrisp
    python generate_notebook.py cirq
"""

import sys
import json
from pathlib import Path


def generate_notebook(framework_name: str, output_number: str = "0X"):
    """Generate notebook for new framework integration.

    Args:
        framework_name: Name of the framework (e.g., 'qrisp', 'cirq')
        output_number: Notebook number prefix (e.g., '03', '04')
    """
    # Paths
    script_dir = Path(__file__).parent
    template_path = script_dir / "templates" / "framework_template.ipynb"
    output_path = script_dir / f"{output_number}_{framework_name}_integration.ipynb"

    # Check if template exists
    if not template_path.exists():
        print(f"Error: Template not found at {template_path}")
        sys.exit(1)

    # Load template
    with open(template_path) as f:
        notebook = json.load(f)

    # Prepare replacements
    framework_title = framework_name.title()
    framework_lower = framework_name.lower()

    replacements = {
        '[FRAMEWORK_NAME]': framework_title,
        '[FRAMEWORK_LOWER]': framework_lower,
        '[framework_lower]': framework_lower,
        '[PACKAGE_REQUIREMENTS]': f'{framework_lower}>=1.0.0',  # Default, adjust as needed
    }

    # Replace placeholders in all cells
    for cell in notebook['cells']:
        if cell['cell_type'] in ['markdown', 'code']:
            if isinstance(cell['source'], list):
                # Handle list of lines
                cell['source'] = [
                    replace_all(line, replacements)
                    for line in cell['source']
                ]
            elif isinstance(cell['source'], str):
                # Handle single string
                cell['source'] = replace_all(cell['source'], replacements)

    # Save generated notebook
    with open(output_path, 'w') as f:
        json.dump(notebook, f, indent=1, ensure_ascii=False)

    print(f"✓ Generated notebook: {output_path}")
    print(f"\nNext steps:")
    print(f"1. Edit {output_path} to add {framework_title}-specific content")
    print(f"2. Fill in the TODO sections with actual {framework_title} code")
    print(f"3. Update package requirements in pyproject.toml:")
    print(f"   {framework_lower} = [\"{framework_lower}>=X.Y.Z\"]")
    print(f"4. Create the integration module:")
    print(f"   mkdir -p python/arvak/integrations/{framework_lower}")
    print(f"5. Implement the FrameworkIntegration class")


def replace_all(text: str, replacements: dict) -> str:
    """Replace all occurrences of keys with values.

    Args:
        text: Text to process
        replacements: Dictionary of replacements

    Returns:
        Processed text
    """
    for old, new in replacements.items():
        text = text.replace(old, new)
    return text


def main():
    """Main entry point."""
    if len(sys.argv) < 2:
        print("Usage: python generate_notebook.py <framework_name> [notebook_number]")
        print("\nExamples:")
        print("  python generate_notebook.py qrisp 03")
        print("  python generate_notebook.py cirq 04")
        print("  python generate_notebook.py pennylane 05")
        sys.exit(1)

    framework_name = sys.argv[1]
    output_number = sys.argv[2] if len(sys.argv) > 2 else "0X"

    generate_notebook(framework_name, output_number)


if __name__ == "__main__":
    main()
