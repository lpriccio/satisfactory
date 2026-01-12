"""Summary tables and metrics display."""

import math

import pandas as pd
import streamlit as st


def render_summary():
    """Render aggregate totals and infrastructure summary."""
    chain = st.session_state.current_chain
    if not chain:
        st.info("Create a build chain to see the summary")
        return

    totals = st.session_state.aggregator.aggregate(chain)

    st.subheader(f"Summary: {chain.name}")

    # Three columns for key metrics
    col1, col2, col3 = st.columns(3)

    with col1:
        power_label = "Power Draw" if totals.total_power >= 0 else "Power Generated"
        st.metric(f"⚡ {power_label}", f"{abs(totals.total_power):.1f} MW")
    with col2:
        st.metric("📐 Total Floor Space", f"{totals.total_floor_space / 64 *3.0:.0f}Fd (@3x)")
    with col3:
        machine_total = sum(totals.machine_counts.values())
        st.metric("🏭 Total Machines", f"{machine_total:.2f}")

    st.divider()

    # Item balance table
    st.subheader("📦 Item Balance")

    balance_data = []
    for item in sorted(totals.net_balance.keys()):
        prod = totals.gross_production.get(item, 0)
        cons = totals.gross_consumption.get(item, 0)
        net = totals.net_balance[item]

        if net > 0.01:
            status = "✅ Surplus"
        elif net < -0.01:
            status = "⚠️ Deficit"
        else:
            status = "⚖️ Balanced"

        balance_data.append(
            {
                "Item": item,
                "Produced": f"{prod:.2f}/min",
                "Consumed": f"{cons:.2f}/min",
                "Net": f"{net:+.2f}/min",
                "Status": status,
            }
        )

    if balance_data:
        df = pd.DataFrame(balance_data)
        st.dataframe(df, use_container_width=True, hide_index=True)
    else:
        st.info("No items in chain")

    st.divider()

    # Machine counts by recipe with subtotals
    st.subheader("🏭 Machine Requirements")

    if totals.machine_counts_by_recipe:
        # Group by building type
        from collections import defaultdict
        by_building: dict[str, list[tuple[str, float]]] = defaultdict(list)
        for (building, recipe), count in totals.machine_counts_by_recipe.items():
            by_building[building].append((recipe, count))

        machine_data = []
        for building in sorted(by_building.keys()):
            recipes = by_building[building]
            # Sort recipes by count descending
            recipes.sort(key=lambda x: -x[1])

            for recipe, count in recipes:
                machine_data.append({
                    "Building": building,
                    "Recipe": recipe,
                    "Count": f"{count:.2f}",
                    "Whole": str(math.ceil(count)),
                })

            # Add subtotal row
            subtotal = totals.machine_counts[building]
            machine_data.append({
                "Building": f"{building} TOTAL",
                "Recipe": "---",
                "Count": f"{subtotal:.2f}",
                "Whole": str(math.ceil(subtotal)),
            })

        st.dataframe(
            pd.DataFrame(machine_data), use_container_width=True, hide_index=True
        )
    else:
        st.info("No machines needed")

    st.divider()

    # Base resources needed
    st.subheader("🪨 Base Resources Required")

    if totals.base_resources:
        resource_data = [
            {"Resource": item, "Rate": f"{rate:.2f}/min"}
            for item, rate in sorted(totals.base_resources.items())
        ]
        st.dataframe(
            pd.DataFrame(resource_data), use_container_width=True, hide_index=True
        )
    else:
        st.success("No external resources needed - fully self-sufficient!")


def render_combine_tab():
    """Render the combine chains tab."""
    st.subheader("🔗 Combine Build Chains")

    storage = st.session_state.storage
    saved_chains = storage.list_chains()

    if len(saved_chains) < 1:
        st.info("Save at least one chain to use the combine feature")
        return

    st.write("Select chains and multipliers to combine:")

    # Store combination selections in session state
    if "combine_selections" not in st.session_state:
        st.session_state.combine_selections = []

    # Add chain selector
    col1, col2, col3 = st.columns([3, 1, 1])
    with col1:
        chain_options = {f"{name} ({target})": path for path, name, target, _ in saved_chains}
        selected_chain = st.selectbox(
            "Chain to add",
            options=list(chain_options.keys()),
            key="combine_chain_select",
        )
    with col2:
        multiplier = st.number_input(
            "Multiplier",
            min_value=0.1,
            value=1.0,
            step=0.5,
            key="combine_multiplier",
        )
    with col3:
        st.write("")  # Spacing
        st.write("")
        if st.button("Add"):
            path = chain_options[selected_chain]
            st.session_state.combine_selections.append((path, multiplier))
            st.rerun()

    # Show current selections
    if st.session_state.combine_selections:
        st.write("**Current combination:**")
        for i, (path, mult) in enumerate(st.session_state.combine_selections):
            chain_name = path.stem
            col1, col2 = st.columns([4, 1])
            with col1:
                st.write(f"{mult}x {chain_name}")
            with col2:
                if st.button("Remove", key=f"remove_combine_{i}"):
                    st.session_state.combine_selections.pop(i)
                    st.rerun()

        st.divider()

        # Calculate and show combined totals
        if st.button("Calculate Combined Totals", type="primary"):
            chains_with_multipliers = []
            for path, mult in st.session_state.combine_selections:
                chain = storage.load(path)
                # Recalculate to ensure consistency
                chain = st.session_state.calculator.recalculate(chain)
                chains_with_multipliers.append((chain, mult))

            combined_totals = st.session_state.aggregator.combine_chains(
                chains_with_multipliers
            )

            # Display combined results
            st.subheader("Combined Results")

            col1, col2, col3 = st.columns(3)
            with col1:
                power_label = (
                    "Power Draw"
                    if combined_totals.total_power >= 0
                    else "Power Generated"
                )
                st.metric(
                    f"⚡ {power_label}", f"{abs(combined_totals.total_power):.1f} MW"
                )
            with col2:
                st.metric(
                    "📐 Floor Space", f"{combined_totals.total_floor_space:.0f} units"
                )
            with col3:
                machine_total = sum(combined_totals.machine_counts.values())
                st.metric("🏭 Machines", f"{machine_total:.2f}")

            # Base resources
            st.write("**Base Resources Required:**")
            if combined_totals.base_resources:
                for item, rate in sorted(combined_totals.base_resources.items()):
                    st.write(f"- {item}: {rate:.2f}/min")
            else:
                st.success("Self-sufficient!")

            # Net balance
            st.write("**Net Balance:**")
            balance_data = []
            for item in sorted(combined_totals.net_balance.keys()):
                net = combined_totals.net_balance[item]
                if abs(net) > 0.01:
                    status = "Surplus" if net > 0 else "Deficit"
                    balance_data.append(
                        {"Item": item, "Net": f"{net:+.2f}/min", "Status": status}
                    )
            if balance_data:
                st.dataframe(
                    pd.DataFrame(balance_data),
                    use_container_width=True,
                    hide_index=True,
                )

        # Save combined as new chain
        st.divider()
        new_name = st.text_input("Save combined as:", value="Combined Chain")
        if st.button("Save as New Chain"):
            # Create a combined chain (we'll use the first chain as base)
            if st.session_state.combine_selections:
                first_path, first_mult = st.session_state.combine_selections[0]
                base_chain = storage.load(first_path)

                # Scale the base chain
                combined_chain = BuildChain(
                    name=new_name,
                    description=f"Combined from: {[p.stem for p, _ in st.session_state.combine_selections]}",
                    target_item=base_chain.target_item,
                    target_rate=base_chain.target_rate * first_mult,
                    recipe_selections=base_chain.recipe_selections.copy(),
                    imported_items=base_chain.imported_items.copy(),
                )

                # Recalculate
                combined_chain = st.session_state.calculator.recalculate(
                    combined_chain
                )
                storage.save(combined_chain)
                st.success(f"Saved '{new_name}'!")
                st.session_state.combine_selections = []
                st.rerun()

        if st.button("Clear Selection"):
            st.session_state.combine_selections = []
            st.rerun()


# Import for type hint
from satisfactory.models.build_chain import BuildChain
