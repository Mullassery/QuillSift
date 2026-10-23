"""ScrapeOwl: deterministic scraping and schema-validated LLM extraction."""

from ._scrapeowl import __version__, crawl, extract, fetch

__all__ = ["__version__", "crawl", "extract", "fetch"]
