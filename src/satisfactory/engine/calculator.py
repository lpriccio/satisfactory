"""Dependency resolution calculator."""

from typing import Optional
from uuid import UUID, uuid4

from satisfactory.data.loader import RecipeDatabase
from satisfactory.models.build_chain import BuildChain, ProductionNode


class DependencyCalculator:
    """Recursively calculates production requirements."""

    def __init__(self, db: RecipeDatabase):
        self.db = db
        self._visited_stack: set[str] = set()  # For cycle detection

    def calculate_chain(
        self,
        target_item: str,
        target_rate: float,
        recipe_selections: dict[str, str],
        speed_multipliers: dict[str, float],
        imported_items: set[str],
        parent_id: Optional[UUID] = None,
    ) -> ProductionNode:
        """
        Recursively build production tree for target item at desired rate.

        Args:
            target_item: Item to produce
            target_rate: Desired items/min output
            recipe_selections: User's recipe choices (item -> recipe_name)
            speed_multipliers: Recipe speed multipliers (recipe_name -> multiplier)
            imported_items: Items marked as imported (stop recursion)
            parent_id: Parent node ID for tree structure
        """
        node = ProductionNode(
            id=uuid4(),
            item_name=target_item,
            target_rate=target_rate,
            parent_id=parent_id,
        )

        # Check if imported
        if target_item in imported_items:
            node.is_imported = True
            return node

        # Check if base resource (no recipes produce it)
        recipes = self.db.get_recipes_for_item(target_item)
        if not recipes:
            node.is_imported = True  # Treat as base resource
            return node

        # Cycle detection for recipes like Recycled Plastic <-> Recycled Rubber
        if target_item in self._visited_stack:
            # Mark as imported to break cycle - user must handle manually
            node.is_imported = True
            return node

        self._visited_stack.add(target_item)

        try:
            # Get selected recipe (or default to first available)
            selected_recipe_name = recipe_selections.get(target_item)
            recipe = None
            if selected_recipe_name:
                recipe = self.db.get_recipe(selected_recipe_name)
                # Verify this recipe actually produces the target item
                if recipe and recipe.get_output_rate(target_item) <= 0:
                    recipe = None

            if not recipe:
                # Find first recipe that produces this item
                for r in recipes:
                    if r.get_output_rate(target_item) > 0:
                        recipe = r
                        break

            if not recipe:
                node.is_imported = True
                return node

            node.recipe_name = recipe.name

            # Get speed multiplier for this recipe (default 1.0 = 100%)
            speed = speed_multipliers.get(recipe.name, 1.0)

            # Calculate machines needed (speed multiplier reduces machine count)
            output_rate = recipe.get_output_rate(target_item) * speed
            if output_rate > 0:
                node.machine_count = target_rate / output_rate
                node.actual_production_rate = target_rate

                # Power and floor space
                node.power_consumption = (
                    node.machine_count * recipe.building.power_draw
                )
                node.floor_space = node.machine_count * recipe.building.floor_space

                # Handle power generators (produce power instead of consuming)
                if recipe.is_power_generator():
                    power_output = recipe.get_output_rate("MW") * speed
                    # Negative = generation
                    node.power_consumption = -node.machine_count * power_output

            # Process inputs recursively (input rate scales with speed)
            for input_io in recipe.inputs:
                input_rate = (
                    recipe.get_input_rate(input_io.item_name) * speed * node.machine_count
                )

                child_node = self.calculate_chain(
                    target_item=input_io.item_name,
                    target_rate=input_rate,
                    recipe_selections=recipe_selections,
                    speed_multipliers=speed_multipliers,
                    imported_items=imported_items,
                    parent_id=node.id,
                )
                node.children.append(child_node)

        finally:
            self._visited_stack.discard(target_item)

        return node

    def recalculate(self, chain: BuildChain) -> BuildChain:
        """Recalculate entire chain with current settings."""
        self._visited_stack.clear()

        chain.root_node = self.calculate_chain(
            target_item=chain.target_item,
            target_rate=chain.target_rate,
            recipe_selections=chain.recipe_selections,
            speed_multipliers=chain.speed_multipliers,
            imported_items=chain.imported_items,
        )
        return chain
