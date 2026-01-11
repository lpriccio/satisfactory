"""TSV parser and recipe database."""

import csv
from collections import defaultdict
from pathlib import Path

from satisfactory.models.recipe import Building, IOType, Recipe, RecipeIO


class RecipeDatabase:
    """Loads and indexes all recipes from TSV."""

    def __init__(self, tsv_path: Path):
        self.recipes: dict[str, Recipe] = {}  # recipe_name -> Recipe
        self.recipes_by_output: dict[str, list[str]] = defaultdict(
            list
        )  # item -> [recipe_names]
        self.all_items: set[str] = set()
        self.buildings: dict[str, Building] = {}

        self._load_recipes(tsv_path)

    def _load_recipes(self, path: Path) -> None:
        """Parse TSV and build recipe index."""
        # Group rows by recipe name
        recipe_rows: dict[str, list[dict]] = defaultdict(list)

        with open(path, "r", encoding="utf-8") as f:
            reader = csv.DictReader(f, delimiter="\t")
            for row in reader:
                recipe_name = row.get("Recipe", "").strip()

                # Skip empty, placeholder, or invalid rows
                if not recipe_name:
                    continue
                if recipe_name == "xxx":
                    continue
                building = row.get("Building", "").strip()
                if not building or building.startswith("#"):
                    continue

                recipe_rows[recipe_name].append(row)

        # Build Recipe objects
        for recipe_name, rows in recipe_rows.items():
            first_row = rows[0]

            # Parse building
            building_name = first_row.get("Building", "").strip()
            if building_name and building_name not in self.buildings:
                try:
                    draw = (
                        float(first_row["Draw"])
                        if first_row.get("Draw")
                        else 0.0
                    )
                    size = (
                        float(first_row["Size"])
                        if first_row.get("Size")
                        else 0.0
                    )
                    self.buildings[building_name] = Building(
                        building_name, draw, size
                    )
                except ValueError:
                    continue

            # Parse runtime
            try:
                runtime = float(first_row.get("Runtime", 0))
                if runtime <= 0:
                    continue
            except (ValueError, TypeError):
                continue

            # Parse inputs and outputs
            inputs = []
            outputs = []

            for row in rows:
                item_name = row.get("Item", "").strip()
                if not item_name:
                    continue

                # Skip rows with missing or zero amount
                amount_str = row.get("Amount", "").strip()
                if not amount_str:
                    continue
                try:
                    amount = float(amount_str)
                except ValueError:
                    continue

                # Skip zero amounts
                if amount == 0:
                    continue

                self.all_items.add(item_name)

                if amount < 0:
                    # Store absolute value for inputs
                    inputs.append(
                        RecipeIO(item_name, abs(amount), IOType.INPUT)
                    )
                else:
                    outputs.append(RecipeIO(item_name, amount, IOType.OUTPUT))

            # Skip recipes with no outputs
            if not outputs:
                continue

            recipe = Recipe(
                name=recipe_name,
                runtime=runtime,
                building=self.buildings.get(
                    building_name, Building(building_name, 0, 0)
                ),
                inputs=tuple(inputs),
                outputs=tuple(outputs),
            )

            self.recipes[recipe_name] = recipe

            # Index by output item
            for output in outputs:
                self.recipes_by_output[output.item_name].append(recipe_name)

    def get_recipes_for_item(self, item_name: str) -> list[Recipe]:
        """Get all recipes that produce a given item."""
        recipe_names = self.recipes_by_output.get(item_name, [])
        return [self.recipes[name] for name in recipe_names if name in self.recipes]

    def get_recipe(self, recipe_name: str) -> Recipe | None:
        """Get a specific recipe by name."""
        return self.recipes.get(recipe_name)

    def get_base_resources(self) -> set[str]:
        """Items that are never produced (true base resources)."""
        produced_items = set(self.recipes_by_output.keys())
        # These are consumed but never produced
        consumed_items = set()
        for recipe in self.recipes.values():
            for inp in recipe.inputs:
                consumed_items.add(inp.item_name)

        return consumed_items - produced_items

    def get_producible_items(self) -> set[str]:
        """All items that can be produced."""
        return set(self.recipes_by_output.keys())

    def get_raw_resources(self) -> set[str]:
        """Items that can only be produced by Converter (effectively raw ores).

        These items should typically be treated as imported/base resources
        unless the user explicitly wants to use Converters.
        """
        raw = set()
        for item, recipe_names in self.recipes_by_output.items():
            recipes = [self.recipes[n] for n in recipe_names if n in self.recipes]
            # If all recipes for this item are Converter recipes, it's a raw resource
            if recipes and all(r.building.name == "Converter" for r in recipes):
                raw.add(item)
        return raw

    def get_default_imported_items(self) -> set[str]:
        """Get items that should be imported by default.

        Includes true base resources and raw ores (Converter-only items).
        """
        return self.get_base_resources() | self.get_raw_resources()

    def get_non_converter_recipes(self, item_name: str) -> list[Recipe]:
        """Get recipes that produce an item, excluding Converter recipes."""
        recipes = self.get_recipes_for_item(item_name)
        return [r for r in recipes if r.building.name != "Converter"]
