"""Tests for flowctl.core.config — deep_merge, config loading."""

import json

import pytest

from flowctl.core.config import deep_merge, get_default_config, load_flow_config


class TestDeepMerge:
    """Tests for deep_merge() — recursive dict merging."""

    def test_flat_merge(self):
        """Simple key-value merge, override wins."""
        base = {"a": 1, "b": 2}
        override = {"b": 3, "c": 4}
        result = deep_merge(base, override)
        assert result == {"a": 1, "b": 3, "c": 4}

    def test_nested_merge(self):
        """Nested dicts should be merged recursively."""
        base = {"level1": {"a": 1, "b": 2}}
        override = {"level1": {"b": 3, "c": 4}}
        result = deep_merge(base, override)
        assert result == {"level1": {"a": 1, "b": 3, "c": 4}}

    def test_deep_nested_merge(self):
        """Multiple levels of nesting should all merge."""
        base = {"l1": {"l2": {"l3": {"a": 1, "b": 2}}}}
        override = {"l1": {"l2": {"l3": {"b": 3}}}}
        result = deep_merge(base, override)
        assert result == {"l1": {"l2": {"l3": {"a": 1, "b": 3}}}}

    def test_override_dict_with_scalar(self):
        """When override provides a scalar for a dict key, scalar wins."""
        base = {"a": {"nested": True}}
        override = {"a": "flat_value"}
        result = deep_merge(base, override)
        assert result == {"a": "flat_value"}

    def test_override_scalar_with_dict(self):
        """When override provides a dict for a scalar key, dict wins."""
        base = {"a": "flat_value"}
        override = {"a": {"nested": True}}
        result = deep_merge(base, override)
        assert result == {"a": {"nested": True}}

    def test_empty_override(self):
        """Empty override should return base unchanged."""
        base = {"a": 1, "b": {"c": 2}}
        result = deep_merge(base, {})
        assert result == base

    def test_empty_base(self):
        """Empty base should return override."""
        override = {"a": 1, "b": {"c": 2}}
        result = deep_merge({}, override)
        assert result == override

    def test_both_empty(self):
        result = deep_merge({}, {})
        assert result == {}

    def test_does_not_mutate_base(self):
        """deep_merge should not mutate the base dict."""
        base = {"a": 1, "b": {"c": 2}}
        original_base = {"a": 1, "b": {"c": 2}}
        deep_merge(base, {"a": 99})
        assert base == original_base

    def test_lists_are_replaced_not_merged(self):
        """Lists should be replaced entirely, not concatenated."""
        base = {"items": [1, 2, 3]}
        override = {"items": [4, 5]}
        result = deep_merge(base, override)
        assert result == {"items": [4, 5]}

    def test_none_override(self):
        """None in override should replace base value."""
        base = {"a": "something"}
        override = {"a": None}
        result = deep_merge(base, override)
        assert result == {"a": None}


class TestGetDefaultConfig:
    def test_has_required_keys(self):
        config = get_default_config()
        assert "memory" in config
        assert "planSync" in config
        assert "review" in config
        assert "scouts" in config
        assert "stack" in config

    def test_memory_enabled_by_default(self):
        config = get_default_config()
        assert config["memory"]["enabled"] is True


class TestLoadFlowConfig:
    def test_returns_defaults_when_no_config(self, flow_dir, monkeypatch):
        """When config.json is missing, should return defaults."""
        # Remove the config file that flow_dir fixture created
        config_path = flow_dir / "config.json"
        config_path.unlink()

        config = load_flow_config()
        defaults = get_default_config()
        assert config == defaults

    def test_merges_with_defaults(self, flow_dir, monkeypatch):
        """Partial config should be merged with defaults."""
        config_path = flow_dir / "config.json"
        partial = {"memory": {"enabled": False}}
        config_path.write_text(json.dumps(partial) + "\n", encoding="utf-8")

        config = load_flow_config()
        # Overridden value
        assert config["memory"]["enabled"] is False
        # Default values still present
        assert "planSync" in config
        assert "review" in config

    def test_handles_invalid_json(self, flow_dir, monkeypatch):
        """Invalid JSON should return defaults."""
        config_path = flow_dir / "config.json"
        config_path.write_text("not json{{{", encoding="utf-8")

        config = load_flow_config()
        defaults = get_default_config()
        assert config == defaults
