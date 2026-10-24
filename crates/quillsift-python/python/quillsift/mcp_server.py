"""MCP server exposing QuillSift's fetch/parse/extract tools.

Install with ``pip install "quillsift[mcp]"`` and run ``quillsift-mcp``. The
server is local-first: it calls the same Python bindings as the library, so
SSRF policy and schema validation are identical.
"""

from __future__ import annotations

import json
from typing import Any

import quillsift


def fetch_url(
    url: str, format: str = "markdown", allow_private: bool = False
) -> Any:
    """Fetch a URL and return markdown, text, or structured JSON."""
    return quillsift.fetch(url, format=format, allow_private=allow_private)


def parse_html_tool(
    html: str,
    base_url: str = "https://quillsift.local/",
    format: str = "markdown",
) -> Any:
    """Parse caller-supplied HTML without network access."""
    return quillsift.parse_html(html, base_url=base_url, format=format)


def extract_url(
    url: str,
    schema: dict[str, Any],
    *,
    provider: str = "openai",
    base_url: str | None = None,
    model: str = "gpt-4o-mini",
    api_key_env: str = "OPENAI_API_KEY",
    allow_private: bool = False,
) -> dict[str, Any]:
    """Fetch a URL and extract schema-validated data with an LLM."""
    schema_arg: dict[str, Any] | str = schema
    if isinstance(schema, str):
        schema_arg = schema
    return quillsift.extract(
        url,
        schema_arg,
        provider=provider,
        base_url=base_url,
        model=model,
        api_key_env=api_key_env,
        allow_private=allow_private,
    )


def crawl_site(
    url: str,
    max_depth: int = 1,
    max_pages: int = 5,
    allow_private: bool = False,
) -> str:
    """Bounded crawl of a site. Returns a JSON string for compact MCP output."""
    report = quillsift.crawl(
        url,
        max_depth=max_depth,
        max_pages=max_pages,
        allow_private=allow_private,
    )
    return json.dumps(report)


def build_server():
    """Build the FastMCP server. Kept separate so tests can import without MCP."""
    try:
        from fastmcp import FastMCP
    except ImportError as exc:
        raise ImportError(
            "The MCP server requires FastMCP. Install with: pip install \"quillsift[mcp]\""
        ) from exc

    mcp = FastMCP("QuillSift")

    @mcp.tool
    def quillsift_fetch(
        url: str, format: str = "markdown", allow_private: bool = False
    ) -> Any:
        """Fetch a URL and return clean markdown, text, or JSON."""
        return fetch_url(url, format=format, allow_private=allow_private)

    @mcp.tool
    def quillsift_parse_html(
        html: str,
        base_url: str = "https://quillsift.local/",
        format: str = "markdown",
    ) -> Any:
        """Parse caller-supplied HTML with no network access."""
        return parse_html_tool(html, base_url=base_url, format=format)

    @mcp.tool
    def quillsift_extract(
        url: str,
        schema: dict[str, Any],
        provider: str = "openai",
        base_url: str | None = None,
        model: str = "gpt-4o-mini",
        api_key_env: str = "OPENAI_API_KEY",
        allow_private: bool = False,
    ) -> dict[str, Any]:
        """Extract schema-validated data from a URL using an LLM."""
        return extract_url(
            url,
            schema,
            provider=provider,
            base_url=base_url,
            model=model,
            api_key_env=api_key_env,
            allow_private=allow_private,
        )

    @mcp.tool
    def quillsift_crawl(
        url: str,
        max_depth: int = 1,
        max_pages: int = 5,
        allow_private: bool = False,
    ) -> str:
        """Bounded crawl; returns a JSON report string."""
        return crawl_site(
            url,
            max_depth=max_depth,
            max_pages=max_pages,
            allow_private=allow_private,
        )

    return mcp


def main() -> None:
    build_server().run()


if __name__ == "__main__":
    main()
