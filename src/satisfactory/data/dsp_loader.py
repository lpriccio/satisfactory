"""TSV parser for Dyson Sphere Program recipes."""

import csv
from collections import defaultdict
from pathlib import Path

from satisfactory.models.recipe import Building, IOType, Recipe, RecipeIO


class DSPRecipeDatabase:
    """Loads and indexes DSP recipes from TSV."""

    def __init__(self, tsv_path: Path):
        self.recipes: dict[str, Recipe] = {}  # recipe_name -> Recipe
        self.recipes_by_output: dict[str, list[str]] = defaultdict(
            list
        )  # item -> [recipe_names]
        self.all_items: set[str] = set()
        # DSP doesn't track buildings in the same way
        self._generic_building = Building("Assembler", 0.0, 0.0)

        self._load_recipes(tsv_path)

    def _load_recipes(self, path: Path) -> None:
        """Parse DSP TSV and build recipe index."""
        # Group rows by recipe name
        recipe_rows: dict[str, list[dict]] = defaultdict(list)

        with open(path, "r", encoding="utf-8") as f:
            reader = csv.DictReader(f, delimiter="\t")
            for row in reader:
                recipe_name = row.get("Recipe", "").strip()
                if not recipe_name:
                    continue
                recipe_rows[recipe_name].append(row)

        # Build Recipe objects
        for recipe_name, rows in recipe_rows.items():
            first_row = rows[0]

            # Parse time (crafting time in seconds)
            try:
                runtime = float(first_row.get("Seconds", 0))
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

                try:
                    amount = float(row.get("Item Count", 0))
                except (ValueError, TypeError):
                    continue

                if amount == 0:
                    continue

                self.all_items.add(item_name)

                if amount < 0:
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
                building=self._generic_building,
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
        consumed_items = set()
        for recipe in self.recipes.values():
            for inp in recipe.inputs:
                consumed_items.add(inp.item_name)

        return consumed_items - produced_items

    def get_producible_items(self) -> set[str]:
        """All items that can be produced."""
        return set(self.recipes_by_output.keys())

    def get_raw_resources(self) -> set[str]:
        """For DSP, returns empty - handled by get_base_resources."""
        return set()

    def get_default_imported_items(self) -> set[str]:
        """Get items that should be imported by default."""
        return self.get_base_resources()

    def get_non_converter_recipes(self, item_name: str) -> list[Recipe]:
        """Get recipes - DSP has no Converter equivalent."""
        return self.get_recipes_for_item(item_name)
