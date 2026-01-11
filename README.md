# Satisfactory Build Chain Planner

A Streamlit application for planning factory build chains in Satisfactory.

## Features

- Recursive dependency resolution for any producible item
- Choose alternate recipes for each component
- Mark items as imported (no production chain needed)
- View hierarchical dependency tree
- Aggregate totals: production, consumption, net balance
- Machine counts, power consumption, floor space
- Save/load build chains
- Combine chains via linear combination

## Setup

```bash
uv sync
uv run streamlit run run.py
```

## Usage

1. Select a target product and desired output rate
2. Configure recipes for each component (or use defaults)
3. Mark items as imported if you're sourcing them externally
4. View the summary for machine counts, power, and resources needed
5. Save your build chain for later
