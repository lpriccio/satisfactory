"""Game mode definitions."""

from enum import Enum


class GameMode(Enum):
    """Supported games."""

    SATISFACTORY = "satisfactory"
    FACTORIO = "factorio"

    @property
    def display_name(self) -> str:
        return self.value.capitalize()

    @property
    def recipe_file(self) -> str:
        if self == GameMode.SATISFACTORY:
            return "recipes.tsv"
        return "recipes_factorio.tsv"

    @property
    def save_folder(self) -> str:
        return self.value

    @property
    def has_power(self) -> bool:
        return self == GameMode.SATISFACTORY

    @property
    def has_floor_space(self) -> bool:
        return self == GameMode.SATISFACTORY

    @property
    def has_buildings(self) -> bool:
        return self == GameMode.SATISFACTORY

    @property
    def has_productivity(self) -> bool:
        return self == GameMode.FACTORIO

    @property
    def background_color(self) -> str:
        if self == GameMode.SATISFACTORY:
            return "#0a1628"  # Dark blue
        return "#1a0a28"  # Deep purple
