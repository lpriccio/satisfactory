"""Main Streamlit application for Factory Build Planner."""

from pathlib import Path

import streamlit as st

from satisfactory.data.dsp_loader import DSPRecipeDatabase
from satisfactory.data.factorio_loader import FactorioRecipeDatabase
from satisfactory.data.loader import RecipeDatabase
from satisfactory.engine.aggregator import ChainAggregator
from satisfactory.engine.calculator import DependencyCalculator
from satisfactory.models.game_mode import GameMode
from satisfactory.persistence.storage import ChainStorage
from satisfactory.ui.components import render_sidebar
from satisfactory.ui.summary_view import render_combine_tab, render_summary
from satisfactory.ui.tree_view import render_dependency_tree

# Paths relative to this file's location
_THIS_DIR = Path(__file__).parent
_PROJECT_ROOT = _THIS_DIR.parent.parent
STORAGE_BASE = _PROJECT_ROOT / "saved_chains"


def _get_data_path(mode: GameMode) -> Path:
    """Get the recipe data file path for a game mode."""
    return _PROJECT_ROOT / mode.recipe_file


def _get_storage_path(mode: GameMode) -> Path:
    """Get the save directory for a game mode."""
    return STORAGE_BASE / mode.save_folder


def init_session_state():
    """Initialize session state variables."""
    # Game mode (default to Satisfactory)
    if "game_mode" not in st.session_state:
        st.session_state.game_mode = GameMode.SATISFACTORY

    mode = st.session_state.game_mode

    # Check if we need to reinitialize for a different game mode
    current_mode_key = f"_initialized_mode"
    if st.session_state.get(current_mode_key) != mode:
        # Clear game-specific state when switching modes
        st.session_state.db = None
        st.session_state.calculator = None
        st.session_state.aggregator = None
        st.session_state.storage = None
        st.session_state.current_chain = None
        st.session_state.prev_target_item = None
        st.session_state.prev_target_rate = None
        st.session_state.chain_name_override = None
        st.session_state[current_mode_key] = mode

    # Initialize database based on game mode
    if st.session_state.get("db") is None:
        data_path = _get_data_path(mode)
        if mode == GameMode.FACTORIO:
            st.session_state.db = FactorioRecipeDatabase(data_path)
        elif mode == GameMode.DSP:
            st.session_state.db = DSPRecipeDatabase(data_path)
        elif mode == GameMode.FOUNDRY:
            # Foundry uses same TSV format as Factorio
            st.session_state.db = FactorioRecipeDatabase(data_path)
        else:
            st.session_state.db = RecipeDatabase(data_path)

    if st.session_state.get("calculator") is None:
        st.session_state.calculator = DependencyCalculator(st.session_state.db)

    if st.session_state.get("aggregator") is None:
        st.session_state.aggregator = ChainAggregator(st.session_state.db)

    if st.session_state.get("storage") is None:
        st.session_state.storage = ChainStorage(_get_storage_path(mode))

    if "current_chain" not in st.session_state:
        st.session_state.current_chain = None


def main():
    """Main application entry point."""
    st.set_page_config(
        page_title="Factory Build Planner",
        page_icon="🏭",
        layout="wide",
    )

    # Game mode selector at very top of sidebar (before init to catch changes)
    with st.sidebar:
        st.header("Game")
        mode_options = list(GameMode)
        current_mode = st.session_state.get("game_mode", GameMode.SATISFACTORY)
        current_idx = mode_options.index(current_mode) if current_mode in mode_options else 0

        selected_mode = st.selectbox(
            "Game Mode",
            options=mode_options,
            index=current_idx,
            format_func=lambda m: m.display_name,
            key="game_mode_select",
            label_visibility="collapsed",
        )

        # Handle mode switch
        if selected_mode != st.session_state.get("game_mode"):
            st.session_state.game_mode = selected_mode
            st.rerun()

        st.divider()

    init_session_state()

    mode = st.session_state.game_mode

    # Inject custom theme CSS based on game mode
    st.markdown(
        f"""
        <style>
            .stApp {{
                background-color: {mode.background_color};
            }}
        </style>
        """,
        unsafe_allow_html=True,
    )

    # Sidebar for chain management (continued)
    with st.sidebar:
        render_sidebar()

    # Main content
    if mode == GameMode.SATISFACTORY:
        icon = "🏭"
    elif mode == GameMode.FACTORIO:
        icon = "⚙️"
    elif mode == GameMode.FOUNDRY:
        icon = "🔨"
    else:  # DSP
        icon = "🌟"
    st.title(f"{icon} {mode.display_name} Build Planner")

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
