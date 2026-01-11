"""Runner script for Streamlit app."""

import sys
from pathlib import Path

# Add src to path for imports
src_path = Path(__file__).parent / "src"
sys.path.insert(0, str(src_path))

from satisfactory.app import main

if __name__ == "__main__":
    main()
