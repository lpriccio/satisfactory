"""Hierarchical dependency tree display with integrated recipe selection."""

import streamlit as st

from satisfactory.models.build_chain import ProductionNode

# Tight column layout CSS
_TIGHT_COLUMNS_CSS = """
<style>
    /* Reduce gap between columns in the tree view */
    div[data-testid="column"] {
        padding-left: 0.2rem !important;
        padding-right: 0.2rem !important;
    }
    /* Make checkbox and button columns minimal width */
    div[data-testid="column"]:has(> div > div > div > input[type="checkbox"]),
    div[data-testid="column"]:has(> div > div > button) {
        flex: 0 0 2.5rem !important;
        min-width: 2.5rem !important;
    }
</style>
"""


def render_dependency_tree():
    """Render hierarchical dependency tree with integrated recipe controls."""
    # Inject CSS for tighter column layout
    st.markdown(_TIGHT_COLUMNS_CSS, unsafe_allow_html=True)

    chain = st.session_state.current_chain
    if not chain or not chain.root_node:
        st.info("Create a build chain to see the dependency tree")
        return

    st.subheader(f"Build Chain: {chain.name}")
    st.caption(f"Target: {chain.target_rate:.2f} {chain.target_item}/min")

    # Track if any changes were made
    if "tree_changed" not in st.session_state:
        st.session_state.tree_changed = False

    # Render tree with integrated controls
    _render_node_with_controls(chain.root_node, chain, depth=0)

    # Apply changes if any were made
    if st.session_state.tree_changed:
        st.session_state.tree_changed = False
        st.session_state.current_chain = st.session_state.calculator.recalculate(
            chain
        )
        st.rerun()


def _render_node_with_controls(node: ProductionNode, chain, depth: int):
    """Render a node with checkbox, item name, recipe dropdown, and speed input inline."""
    db = st.session_state.db

    # Get available recipes for this item
    recipes = db.get_recipes_for_item(node.item_name)
    has_recipes = len(recipes) > 0

    # Indent using columns with empty spacer
    indent_size = depth * 2  # percentage of width for indent
    if indent_size > 0:
        cols = st.columns([indent_size, 100 - indent_size])
        container = cols[1]
    else:
        container = st.container()

    with container:
        if has_recipes:
            # Has recipes - show checkbox, item, recipe dropdown, speed
            is_imported = chain.is_path_imported(node.path)

            if is_imported:
                # Imported: [checkbox] [all btn] Item — import X/min
                col1, col1b, col2 = st.columns([0.5, 0.5, 10], gap="small")
                with col1:
                    new_imported = st.checkbox(
                        "imp",
                        value=True,
                        key=f"import_{node.id}",
                        label_visibility="collapsed",
                    )
                    if not new_imported:
                        chain.set_node_import(node.path, False)
                        st.session_state.tree_changed = True
                with col1b:
                    if st.button("∀", key=f"all_{node.id}", help=f"Produce ALL {node.item_name}"):
                        # Currently imported → produce all
                        chain.set_item_import(node.item_name, False)
                        st.session_state.tree_changed = True
                with col2:
                    st.markdown(f"**{node.item_name}** — 📦 *import {node.target_rate:.2f}/min*")
            else:
                # Producing: [checkbox] [all btn] Item — Xx | [recipe] | [speed]
                col1, col1b, col2, col3, col4 = st.columns([0.5, 0.5, 4, 4, 2], gap="small")

                with col1:
                    new_imported = st.checkbox(
                        "imp",
                        value=False,
                        key=f"import_{node.id}",
                        label_visibility="collapsed",
                    )
                    if new_imported:
                        chain.set_node_import(node.path, True)
                        st.session_state.tree_changed = True

                with col1b:
                    if st.button("∀", key=f"all_{node.id}", help=f"Import ALL {node.item_name}"):
                        # Currently producing → import all
                        chain.set_item_import(node.item_name, True)
                        st.session_state.tree_changed = True

                with col2:
                    st.markdown(f"**{node.item_name}** — 🏭 {node.machine_count:.2f}x")

                with col3:
                    if len(recipes) > 1:
                        recipe_names = [r.name for r in recipes]
                        current = chain.recipe_selections.get(node.item_name, recipes[0].name)
                        if current not in recipe_names:
                            current = recipe_names[0]

                        new_selection = st.selectbox(
                            "recipe",
                            options=recipe_names,
                            index=recipe_names.index(current),
                            key=f"recipe_{node.id}",
                            label_visibility="collapsed",
                        )
                        if new_selection != chain.recipe_selections.get(node.item_name):
                            chain.recipe_selections[node.item_name] = new_selection
                            st.session_state.tree_changed = True
                    else:
                        st.caption(f"{node.recipe_name}")

                with col4:
                    # Speed multiplier (100% = 1.0)
                    current_speed = chain.speed_multipliers.get(node.recipe_name, 1.0)
                    new_speed = st.number_input(
                        "speed",
                        min_value=0.01,
                        max_value=2.5,
                        value=current_speed,
                        step=0.25,
                        format="%.2f",
                        key=f"speed_{node.id}",
                        label_visibility="collapsed",
                    )
                    if abs(new_speed - current_speed) > 0.001:
                        chain.speed_multipliers[node.recipe_name] = new_speed
                        st.session_state.tree_changed = True
        else:
            # No recipes - base resource, just show as imported
            col1, col2 = st.columns([1, 10], gap="small")
            with col1:
                st.markdown("📦")
            with col2:
                st.markdown(f"**{node.item_name}** — *{node.target_rate:.2f}/min needed*")

    # Render children
    for child in node.children:
        _render_node_with_controls(child, chain, depth + 1)
