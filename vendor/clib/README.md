# Shared native C support

The vendored libwebp calls Rust libm for inexact math and a stable Rust sort for qsort.
The forced-include header declares these replacements. This avoids platform differences
in those functions; native allocation and other C runtime functions use the system runtime.

Derived from pngdeer under its MIT license; see the repository LICENSE.
