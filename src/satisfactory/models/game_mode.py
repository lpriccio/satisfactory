"""Game mode definitions."""

from enum import Enum


class GameMode(Enum):
    """Supported games."""

    SATISFACTORY = "satisfactory"
    FACTORIO = "factorio"
    DSP = "dsp"

    @property
    def display_name(self) -> str:
        if self == GameMode.DSP:
            return "Dyson Sphere Program"
        return self.value.capitalize()

    @property
    def recipe_file(self) -> str:
        if self == GameMode.SATISFACTORY:
            return "recipes.tsv"
        elif self == GameMode.FACTORIO:
            return "recipes_factorio.tsv"
        return "recipes_dsp.tsv"

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
        # Factorio has productivity modules, DSP has proliferator
        return self in (GameMode.FACTORIO, GameMode.DSP)

    @property
    def background_color(self) -> str:
        if self == GameMode.SATISFACTORY:
            return "#0a1628"  # Dark blue
        elif self == GameMode.FACTORIO:
            return "#1a0a28"  # Deep purple
        return "#0a2820"  # Dark teal for DSP
