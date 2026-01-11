"""JSON storage for build chains."""

import json
from datetime import datetime
from pathlib import Path
from typing import Optional

from satisfactory.models.build_chain import BuildChain


class ChainStorage:
    """Handles saving/loading build chains to JSON files."""

    def __init__(self, storage_dir: Path):
        self.storage_dir = storage_dir
        self.storage_dir.mkdir(parents=True, exist_ok=True)

    def save(self, chain: BuildChain, filename: Optional[str] = None) -> Path:
        """Save chain to JSON file."""
        chain.updated_at = datetime.now().isoformat()
        if not chain.created_at:
            chain.created_at = chain.updated_at

        if not filename:
            # Sanitize chain name for filename
            safe_name = "".join(
                c if c.isalnum() or c in "-_" else "_" for c in chain.name
            )
            filename = f"{safe_name}_{chain.id}.json"

        filepath = self.storage_dir / filename

        with open(filepath, "w", encoding="utf-8") as f:
            json.dump(chain.to_dict(), f, indent=2)

        return filepath

    def load(self, filepath: Path) -> BuildChain:
        """Load chain from JSON file."""
        with open(filepath, "r", encoding="utf-8") as f:
            data = json.load(f)
        return BuildChain.from_dict(data)

    def list_chains(self) -> list[tuple[Path, str, str, float]]:
        """List all saved chains as (path, name, target_item, target_rate)."""
        chains = []
        for filepath in self.storage_dir.glob("*.json"):
            try:
                with open(filepath, "r", encoding="utf-8") as f:
                    data = json.load(f)
                chains.append(
                    (
                        filepath,
                        data.get("name", "Unnamed"),
                        data.get("target_item", ""),
                        data.get("target_rate", 0.0),
                    )
                )
            except (json.JSONDecodeError, KeyError):
                continue
        return chains

    def delete(self, filepath: Path) -> bool:
        """Delete a saved chain."""
        try:
            filepath.unlink()
            return True
        except FileNotFoundError:
            return False
