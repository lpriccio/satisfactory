"""Build chain and production node models."""

from dataclasses import dataclass, field
from typing import Optional
from uuid import UUID, uuid4


@dataclass
class ProductionNode:
    """Single production step in a build chain."""

    id: UUID = field(default_factory=uuid4)
    item_name: str = ""
    recipe_name: str = ""  # Selected recipe for this item
    target_rate: float = 0.0  # Desired output rate (items/min)
    machine_count: float = 0.0  # Fractional machines needed
    is_imported: bool = False  # If True, no production chain needed
    path: tuple[str, ...] = ()  # Path from root (tuple of item names)

    # Calculated fields
    actual_production_rate: float = 0.0
    power_consumption: float = 0.0
    floor_space: float = 0.0

    # Tree structure
    children: list["ProductionNode"] = field(default_factory=list)
    parent_id: Optional[UUID] = None

    def to_dict(self) -> dict:
        """Serialize for JSON storage."""
        return {
            "id": str(self.id),
            "item_name": self.item_name,
            "recipe_name": self.recipe_name,
            "target_rate": self.target_rate,
            "machine_count": self.machine_count,
            "is_imported": self.is_imported,
            "path": list(self.path),
            "actual_production_rate": self.actual_production_rate,
            "power_consumption": self.power_consumption,
            "floor_space": self.floor_space,
            "children": [c.to_dict() for c in self.children],
            "parent_id": str(self.parent_id) if self.parent_id else None,
        }

    @classmethod
    def from_dict(cls, data: dict) -> "ProductionNode":
        """Deserialize from JSON."""
        node = cls(
            id=UUID(data["id"]),
            item_name=data["item_name"],
            recipe_name=data.get("recipe_name", ""),
            target_rate=data.get("target_rate", 0.0),
            machine_count=data.get("machine_count", 0.0),
            is_imported=data.get("is_imported", False),
            path=tuple(data.get("path", [])),
            actual_production_rate=data.get("actual_production_rate", 0.0),
            power_consumption=data.get("power_consumption", 0.0),
            floor_space=data.get("floor_space", 0.0),
            parent_id=UUID(data["parent_id"]) if data.get("parent_id") else None,
        )
        node.children = [cls.from_dict(c) for c in data.get("children", [])]
        return node


@dataclass
class BuildChain:
    """Complete build chain configuration."""

    id: UUID = field(default_factory=uuid4)
    name: str = ""
    description: str = ""
    target_item: str = ""
    target_rate: float = 0.0  # Root desired rate (items/min)
    root_node: Optional[ProductionNode] = None

    # Recipe choices for each item (item_name -> recipe_name)
    recipe_selections: dict[str, str] = field(default_factory=dict)

    # Speed multipliers for recipes (recipe_name -> multiplier, default 1.0)
    speed_multipliers: dict[str, float] = field(default_factory=dict)

    # Productivity multipliers for recipes (recipe_name -> multiplier, default 1.0)
    # Only affects outputs, not inputs (Factorio-specific)
    productivity_multipliers: dict[str, float] = field(default_factory=dict)

    # Items marked as imported (no production chain) - item-level default
    imported_items: set[str] = field(default_factory=set)

    # Per-node import overrides by path (tuple of item names from root)
    # True = imported, False = not imported (overrides imported_items)
    imported_node_overrides: dict[tuple[str, ...], bool] = field(default_factory=dict)

    # Metadata
    created_at: str = ""
    updated_at: str = ""

    def is_path_imported(self, path: tuple[str, ...]) -> bool:
        """Check if a node path should be imported (considering overrides)."""
        if path in self.imported_node_overrides:
            return self.imported_node_overrides[path]
        # Fall back to item-level import status
        return path[-1] in self.imported_items if path else False

    def set_node_import(self, path: tuple[str, ...], imported: bool) -> None:
        """Set import status for a specific node (creates override)."""
        item_name = path[-1] if path else ""
        item_default = item_name in self.imported_items
        if imported == item_default:
            # Matches default, remove override if present
            self.imported_node_overrides.pop(path, None)
        else:
            # Override needed
            self.imported_node_overrides[path] = imported

    def set_item_import(self, item_name: str, imported: bool) -> None:
        """Set import status for ALL nodes of an item type (clears overrides)."""
        if imported:
            self.imported_items.add(item_name)
        else:
            self.imported_items.discard(item_name)
        # Clear all per-node overrides for this item
        paths_to_remove = [p for p in self.imported_node_overrides if p and p[-1] == item_name]
        for p in paths_to_remove:
            del self.imported_node_overrides[p]

    def to_dict(self) -> dict:
        """Serialize for JSON storage."""
        return {
            "id": str(self.id),
            "name": self.name,
            "description": self.description,
            "target_item": self.target_item,
            "target_rate": self.target_rate,
            "root_node": self.root_node.to_dict() if self.root_node else None,
            "recipe_selections": self.recipe_selections,
            "speed_multipliers": self.speed_multipliers,
            "productivity_multipliers": self.productivity_multipliers,
            "imported_items": list(self.imported_items),
            "imported_node_overrides": {
                "|".join(path): val
                for path, val in self.imported_node_overrides.items()
            },
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        }

    @classmethod
    def from_dict(cls, data: dict) -> "BuildChain":
        """Deserialize from JSON."""
        chain = cls(
            id=UUID(data["id"]),
            name=data["name"],
            description=data.get("description", ""),
            target_item=data["target_item"],
            target_rate=data["target_rate"],
            recipe_selections=data.get("recipe_selections", {}),
            speed_multipliers=data.get("speed_multipliers", {}),
            productivity_multipliers=data.get("productivity_multipliers", {}),
            imported_items=set(data.get("imported_items", [])),
            imported_node_overrides={
                tuple(key.split("|")): val
                for key, val in data.get("imported_node_overrides", {}).items()
            },
            created_at=data.get("created_at", ""),
            updated_at=data.get("updated_at", ""),
        )
        if data.get("root_node"):
            chain.root_node = ProductionNode.from_dict(data["root_node"])
        return chain


@dataclass
class AggregatedTotals:
    """Summary of a build chain's resource requirements."""

    # Item -> gross production (total made, items/min)
    gross_production: dict[str, float] = field(default_factory=dict)

    # Item -> gross consumption (total used internally, items/min)
    gross_consumption: dict[str, float] = field(default_factory=dict)

    # Item -> net balance (production - consumption, negative = needs import)
    net_balance: dict[str, float] = field(default_factory=dict)

    # Building type -> count (totals)
    machine_counts: dict[str, float] = field(default_factory=dict)

    # (Building type, recipe name) -> count (detailed breakdown)
    machine_counts_by_recipe: dict[tuple[str, str], float] = field(default_factory=dict)

    # Total power consumption (MW), can be negative if net producer
    total_power: float = 0.0

    # Total floor space
    total_floor_space: float = 0.0

    # Base resources needed (items with net negative balance)
    base_resources: dict[str, float] = field(default_factory=dict)
