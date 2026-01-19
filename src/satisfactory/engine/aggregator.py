"""Aggregator for computing build chain totals."""

from satisfactory.data.loader import RecipeDatabase
from satisfactory.models.build_chain import AggregatedTotals, BuildChain, ProductionNode


class ChainAggregator:
    """Calculates aggregate totals for a build chain."""

    def __init__(self, db: RecipeDatabase):
        self.db = db

    def aggregate(self, chain: BuildChain) -> AggregatedTotals:
        """Calculate all totals for the chain."""
        totals = AggregatedTotals()

        if not chain.root_node:
            return totals

        self._aggregate_node(chain.root_node, totals)

        # Calculate net balance
        all_items = set(totals.gross_production.keys()) | set(
            totals.gross_consumption.keys()
        )
        for item in all_items:
            prod = totals.gross_production.get(item, 0.0)
            cons = totals.gross_consumption.get(item, 0.0)
            net = prod - cons
            totals.net_balance[item] = net

            # Identify base resources (net negative, needs import)
            if net < -0.001:  # Small tolerance for floating point
                totals.base_resources[item] = abs(net)

        return totals

    def _aggregate_node(
        self, node: ProductionNode, totals: AggregatedTotals
    ) -> None:
        """Recursively aggregate a node and its children."""
        if node.is_imported:
            # Track as base resource consumption
            totals.gross_consumption[node.item_name] = (
                totals.gross_consumption.get(node.item_name, 0.0)
            )
            return

        recipe = self.db.get_recipe(node.recipe_name)
        if not recipe:
            return

        # Get multipliers from node (default 1.0 for backwards compat)
        speed = node.speed_multiplier
        productivity = node.productivity_multiplier

        # Track production (all outputs, including byproducts)
        # Production scales with speed AND productivity
        for output in recipe.outputs:
            if output.item_name == "MW":
                continue  # Power tracked separately
            output_rate = recipe.get_output_rate(output.item_name) * speed * productivity * node.machine_count
            totals.gross_production[output.item_name] = (
                totals.gross_production.get(output.item_name, 0.0) + output_rate
            )

        # Track consumption (inputs used by this node)
        # Consumption scales with speed only (NOT productivity)
        for input_io in recipe.inputs:
            input_rate = recipe.get_input_rate(input_io.item_name) * speed * node.machine_count
            totals.gross_consumption[input_io.item_name] = (
                totals.gross_consumption.get(input_io.item_name, 0.0) + input_rate
            )

        # Track machines by building type
        building_name = recipe.building.name
        totals.machine_counts[building_name] = (
            totals.machine_counts.get(building_name, 0.0) + node.machine_count
        )

        # Track machines by (building, recipe)
        key = (building_name, node.recipe_name)
        totals.machine_counts_by_recipe[key] = (
            totals.machine_counts_by_recipe.get(key, 0.0) + node.machine_count
        )

        # Track power
        totals.total_power += node.power_consumption

        # Track floor space
        totals.total_floor_space += node.floor_space

        # Recurse to children
        for child in node.children:
            self._aggregate_node(child, totals)

    def combine_chains(
        self,
        chains: list[tuple[BuildChain, float]],  # (chain, multiplier)
    ) -> AggregatedTotals:
        """
        Combine multiple chains with scaling factors (linear combination).

        Example: 2x Computer Chain + 1x Motor Chain
        """
        combined = AggregatedTotals()

        for chain, multiplier in chains:
            chain_totals = self.aggregate(chain)

            # Scale and add
            for item, rate in chain_totals.gross_production.items():
                combined.gross_production[item] = (
                    combined.gross_production.get(item, 0.0) + rate * multiplier
                )

            for item, rate in chain_totals.gross_consumption.items():
                combined.gross_consumption[item] = (
                    combined.gross_consumption.get(item, 0.0) + rate * multiplier
                )

            for building, count in chain_totals.machine_counts.items():
                combined.machine_counts[building] = (
                    combined.machine_counts.get(building, 0.0) + count * multiplier
                )

            for key, count in chain_totals.machine_counts_by_recipe.items():
                combined.machine_counts_by_recipe[key] = (
                    combined.machine_counts_by_recipe.get(key, 0.0) + count * multiplier
                )

            combined.total_power += chain_totals.total_power * multiplier
            combined.total_floor_space += chain_totals.total_floor_space * multiplier

        # Recalculate net balance
        all_items = set(combined.gross_production.keys()) | set(
            combined.gross_consumption.keys()
        )
        for item in all_items:
            prod = combined.gross_production.get(item, 0.0)
            cons = combined.gross_consumption.get(item, 0.0)
            combined.net_balance[item] = prod - cons
            if prod - cons < -0.001:
                combined.base_resources[item] = abs(prod - cons)

        return combined
