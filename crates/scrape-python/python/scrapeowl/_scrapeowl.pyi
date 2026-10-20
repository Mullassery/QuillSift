from typing import Any, Dict, Literal, Optional, Union

__version__: str

def fetch(
    url: str,
    *,
    format: Literal["markdown", "text", "json"] = "markdown",
    allow_private: bool = False,
) -> Union[str, Dict[str, Any]]: ...

def extract(
    url: str,
    schema: Union[str, Dict[str, Any]],
    *,
    provider: Literal["openai", "ollama"] = "openai",
    base_url: Optional[str] = None,
    model: str,
    api_key_env: str = "OPENAI_API_KEY",
    allow_private: bool = False,
    max_attempts: int = 3,
    temperature: float = 0.0,
    max_input_chars: int = 100000,
) -> Dict[str, Any]: ...
