# UFP formatter

Run `ufp fmt file.ufp` to format a file in place, or `ufp fmt file.ufp --check`
to return a nonzero status if formatting would change it. Formatting first
validates the source with the syntax AST parser; malformed input is reported
without being rewritten.

The formatter uses four-space indentation and normalizes whitespace and
punctuation in declarations, including terminals, portals, procedures, and
matrix declarations/literals. It is deterministic and idempotent.

**Comment fallback:** comments are currently discarded by the lexer, which
does not expose trivia spans. To avoid dropping or relocating comments,
`ufp fmt` returns any source containing `//` or `/*` in its inter-token trivia
unchanged. Comment formatting is unsupported until comment trivia is represented
in the token API.
