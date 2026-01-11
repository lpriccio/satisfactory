"""Recipe and building data models."""

from dataclasses import dataclass
from enum import Enum


class IOType(Enum):
    """Type of recipe input/output."""

    INPUT = "input"
    OUTPUT = "output"


@dataclass(frozen=True)
class Building:
    """Represents a production building type."""

    name: str
    power_draw: float  # MW consumed (0 for generators)
    floor_space: float  # Size in units


@dataclass(frozen=True)
class RecipeIO:
    """Single input or output of a recipe."""

    item_name: str
    amount: float  # Positive for output, negative for input (absolute stored)
    io_type: IOType


@dataclass(frozen=True)
class Recipe:
    """Complete recipe with all inputs and outputs."""

    name: str
    runtime: float  # Seconds per cycle
    building: Building
    inputs: tuple[RecipeIO, ...]  # Frozen for hashability
    outputs: tuple[RecipeIO, ...]  # May have multiple (byproducts)

    @property
    def cycles_per_minute(self) -> float:
        """Calculate cycles per minute based on runtime."""
        if self.runtime <= 0:
            return 0.0
        return 60.0 / self.runtime

    def get_input_rate(self, item_name: str) -> float:
        """Get consumption rate (items/min) for an input item."""
        for io in self.inputs:
            if io.item_name == item_name:
                return io.amount * self.cycles_per_minute
        return 0.0

    def get_output_rate(self, item_name: str) -> float:
        """Get production rate (items/min) for an output item."""
        for io in self.outputs:
            if io.item_name == item_name:
                return io.amount * self.cycles_per_minute
        return 0.0

    def get_primary_output(self) -> str:
        """Returns the item this recipe is primarily named for (largest output)."""
        if not self.outputs:
            return ""
        return max(self.outputs, key=lambda o: o.amount).item_name

    def is_power_generator(self) -> bool:
        """Check if recipe produces power (MW output)."""
        return any(o.item_name == "MW" for o in self.outputs)
