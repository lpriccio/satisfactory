"""Reusable Streamlit UI components."""

import streamlit as st

from satisfactory.models.build_chain import BuildChain


def _get_default_chain_name(target_item: str, storage) -> str:
    """Generate default chain name like 'Product 1', 'Product 2', etc."""
    existing_names = {name for _, name, _, _ in storage.list_chains()}
    suffix = 1
    while f"{target_item} {suffix}" in existing_names:
        suffix += 1
    return f"{target_item} {suffix}"


def _create_or_update_chain(target_item: str, target_rate: float, chain_name: str):
    """Create or update the current chain."""
    db = st.session_state.db
    default_imports = db.get_default_imported_items()

    # Preserve recipe selections and import overrides if same target
    current = st.session_state.current_chain
    if current and current.target_item == target_item:
        recipe_selections = current.recipe_selections
        speed_multipliers = current.speed_multipliers
        productivity_multipliers = current.productivity_multipliers
        imported_items = current.imported_items
    else:
        recipe_selections = {}
        speed_multipliers = {}
        productivity_multipliers = {}
        imported_items = default_imports

    new_chain = BuildChain(
        name=chain_name,
        target_item=target_item,
        target_rate=target_rate,
        recipe_selections=recipe_selections,
        speed_multipliers=speed_multipliers,
        productivity_multipliers=productivity_multipliers,
        imported_items=imported_items,
    )
    st.session_state.current_chain = st.session_state.calculator.recalculate(
        new_chain
    )


def render_sidebar():
    """Render sidebar with chain management."""
    st.header("Chain Management")

    db = st.session_state.db
    storage = st.session_state.storage

    # Initialize tracking state
    if "prev_target_item" not in st.session_state:
        st.session_state.prev_target_item = None
    if "prev_target_rate" not in st.session_state:
        st.session_state.prev_target_rate = None
    if "chain_name_override" not in st.session_state:
        st.session_state.chain_name_override = None
    if "widget_key_version" not in st.session_state:
        st.session_state.widget_key_version = 0

    # Saved chains section first (so loading can set defaults)
    st.subheader("Saved Chains")

    saved_chains = storage.list_chains()
    if saved_chains:
        # Build options for dropdown
        chain_options = {"(New Chain)": None}
        for filepath, name, target, rate in saved_chains:
            label = f"{name} ({target} @ {rate:.1f}/min)"
            chain_options[label] = filepath

        col1, col2 = st.columns([4, 1])
        with col1:
            selected_label = st.selectbox(
                "Load Chain",
                options=list(chain_options.keys()),
                key=f"load_chain_select_{st.session_state.widget_key_version}",
                label_visibility="collapsed",
            )
        with col2:
            selected_path = chain_options[selected_label]
            if selected_path and st.button("X", key="delete_selected"):
                storage.delete(selected_path)
                st.session_state.widget_key_version += 1
                st.rerun()

        # Handle loading when selection changes
        if selected_label != "(New Chain)" and chain_options[selected_label]:
            filepath = chain_options[selected_label]
            # Check if we need to load this chain
            current = st.session_state.current_chain
            if not current or current.name != selected_label.split(" (")[0]:
                loaded = storage.load(filepath)
                # Update tracking state
                st.session_state.prev_target_item = loaded.target_item
                st.session_state.prev_target_rate = loaded.target_rate
                st.session_state.chain_name_override = loaded.name
                st.session_state.widget_key_version += 1
                # Recalculate and set
                st.session_state.current_chain = (
                    st.session_state.calculator.recalculate(loaded)
                )
                st.rerun()
    else:
        st.info("No saved chains yet")

    st.divider()

    # New/Edit chain section
    st.subheader("Configure Chain")

    producible_items = sorted(db.get_producible_items())

    # Determine default values from current chain or defaults
    if st.session_state.prev_target_item and st.session_state.prev_target_item in producible_items:
        default_item_idx = producible_items.index(st.session_state.prev_target_item)
    else:
        default_item_idx = 0

    default_rate = st.session_state.prev_target_rate or 10.0

    # Target product selection
    target_item = st.selectbox(
        "Target Product",
        options=producible_items,
        index=default_item_idx,
        key=f"target_item_select_{st.session_state.widget_key_version}",
    )

    # Target rate
    target_rate = st.number_input(
        "Output Rate (items/min)",
        min_value=0.1,
        value=float(default_rate),
        step=1.0,
        key=f"target_rate_input_{st.session_state.widget_key_version}",
    )

    # Detect if target item changed - reset chain name
    if target_item != st.session_state.prev_target_item:
        st.session_state.chain_name_override = None

    # Chain name
    if st.session_state.chain_name_override is not None:
        default_name = st.session_state.chain_name_override
    else:
        default_name = _get_default_chain_name(target_item, storage)

    chain_name = st.text_input(
        "Chain Name",
        value=default_name,
        key=f"chain_name_input_{st.session_state.widget_key_version}",
    )

    # Track if user manually changed the name
    if chain_name != default_name:
        st.session_state.chain_name_override = chain_name

    # Auto-create/update chain when target or rate changes
    needs_update = (
        target_item != st.session_state.prev_target_item
        or target_rate != st.session_state.prev_target_rate
    )

    if needs_update:
        st.session_state.prev_target_item = target_item
        st.session_state.prev_target_rate = target_rate
        _create_or_update_chain(target_item, target_rate, chain_name)
        st.rerun()

    # Update chain name if changed
    if st.session_state.current_chain and st.session_state.current_chain.name != chain_name:
        st.session_state.current_chain.name = chain_name

    st.divider()

    # Save current chain
    if st.session_state.current_chain:
        if st.button("Save Current Chain"):
            storage.save(st.session_state.current_chain)
            st.session_state.widget_key_version += 1
            st.success("Saved!")
            st.rerun()
