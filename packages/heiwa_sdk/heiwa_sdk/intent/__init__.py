"""Intent parsing: free text -> structured, deterministic primitives.

No LLM calls live in this package. Everything here is a pure function the
shell can subprocess into; DREX routing only enters when these parsers
return low confidence.
"""

from .parse_time import ParsedTime, parse_time

__all__ = ["ParsedTime", "parse_time"]
