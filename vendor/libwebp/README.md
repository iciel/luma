# Vendored libwebp 1.6.0

Source: https://github.com/webmproject/libwebp/tree/v1.6.0
The original release archive SHA-256 is
93a852c2b3efafee3723efd4636de855b46f9fe1efddd607e1f42f60fc8f2136.

The retained decoder, encoder and sharpyuv sources are unmodified. SIMD sources,
command-line tools and mux/demux are omitted. An empty config.h selects plain C
without threads. The Rust wrapper and C glue originate from pngdeer.

MSVC uses /fp:precise without /fp:contract; GCC/Clang use -ffp-contract=off.
MSVC /fp:strict rejects sharpyuv's static division initializer (C2099).
The shared header routes inexact math and qsort through vendor/clib.
The wrapper initializes encoder/decoder tables once before concurrent use.

Licenses: libwebp/COPYING and libwebp/PATENTS for libwebp;
the repository LICENSE for the wrapper and glue.
