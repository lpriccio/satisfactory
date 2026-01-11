"""Main Streamlit application for Satisfactory Build Planner."""

from pathlib import Path

import streamlit as st

from satisfactory.data.loader import RecipeDatabase
from satisfactory.engine.aggregator import ChainAggregator
from satisfactory.engine.calculator import DependencyCalculator
from satisfactory.persistence.storage import ChainStorage
from satisfactory.ui.components import render_sidebar
from satisfactory.ui.summary_view import render_combine_tab, render_summary
from satisfactory.ui.tree_view import render_dependency_tree

# Paths relative to this file's location
_THIS_DIR = Path(__file__).parent
_PROJECT_ROOT = _THIS_DIR.parent.parent
DATA_PATH = _PROJECT_ROOT / "recipes.tsv"
STORAGE_PATH = _PROJECT_ROOT / "saved_chains"


def init_session_state():
    """Initialize session state variables."""
    if "db" not in st.session_state:
        st.session_state.db = RecipeDatabase(DATA_PATH)
    if "calculator" not in st.session_state:
        st.session_state.calculator = DependencyCalculator(st.session_state.db)
    if "aggregator" not in st.session_state:
        st.session_state.aggregator = ChainAggregator(st.session_state.db)
    if "storage" not in st.session_state:
        st.session_state.storage = ChainStorage(STORAGE_PATH)
    if "current_chain" not in st.session_state:
        st.session_state.current_chain = None


def main():
    """Main application entry point."""
    st.set_page_config(
        page_title="Satisfactory Build Planner",
        page_icon="🏭",
        layout="wide",
    )

    init_session_state()

    # Sidebar for chain management
    with st.sidebar:
        render_sidebar()

    # Main content
    st.title("🏭 Satisfactory Factory Build Planner")

    # Show some stats about loaded data
    db = st.session_state.db
    st.caption(
        f"Loaded {len(db.recipes)} recipes producing {len(db.get_producible_items())} items"
    )

    # Tabs for different views
    tab1, tab2, tab3 = st.tabs(
        ["🎯 Build Chain", "📊 Summary", "🔗 Combine Chains"]
    )

    with tab1:
        render_dependency_tree()

    with tab2:
        render_summary()

    with tab3:
        render_combine_tab()


if __name__ == "__main__":
    main()
