"""Deprecated compatibility shim for the renamed QuillSift package."""

import warnings

warnings.warn(
    "The 'scrapeowl' package was renamed to 'quillsift'. "
    "Install and import 'quillsift' instead.",
    DeprecationWarning,
    stacklevel=2,
)

from quillsift import *  # noqa: F401,F403,E402
from quillsift import __all__ as _quillsift_all  # noqa: E402
from quillsift import __version__  # noqa: F401,E402

__all__ = list(_quillsift_all)
