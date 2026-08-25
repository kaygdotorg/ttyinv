from __future__ import annotations

import yaml


MAX_YAML_DEPTH = 64  # Reject YAML nesting deeper than 64 container levels.


class StringDateSafeLoader(yaml.SafeLoader):
    """Load YAML safely while preserving timestamp-shaped scalars as text."""


def _construct_timestamp_as_text(loader: StringDateSafeLoader, node: yaml.Node) -> str:
    return loader.construct_scalar(node)


StringDateSafeLoader.add_constructor(
    "tag:yaml.org,2002:timestamp",
    _construct_timestamp_as_text,
)
