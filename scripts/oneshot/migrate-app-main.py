#!/usr/bin/env python3
"""
Mechanical Phase 112.C.3 migration:

- `void app_main(void)` → `int nros_app_main(int argc, char **argv)`
- `extern "C" void app_main(void)` → `int nros_app_main(int argc, char **argv)`
- Add `(void)argc; (void)argv;` immediately after opening brace
- Add `#include <nros/app_main.h>` near other `nros/` headers
- Append `NROS_APP_MAIN_REGISTER_VOID()` (or _ZEPHYR / _POSIX based on flag)
- For C void→int conversion: `NROS_CHECK(call)` → `NROS_CHECK_RET(call, 1)`
- For C++ void→int conversion: `NROS_CHECK(call)` → `NROS_TRY_RET(call, 1)`

Idempotent: skips files already migrated (checks for `nros_app_main` symbol).
"""

import re
import sys
from pathlib import Path

def migrate(path: Path, shim: str) -> bool:
    """Returns True if file was modified."""
    text = path.read_text()
    if 'nros_app_main' in text:
        return False  # already migrated

    is_cpp = path.suffix == '.cpp'

    # 1. Add include (place alongside other nros/ includes)
    if '#include <nros/app_main.h>' not in text:
        # Find first "#include <nros/..." line and insert before it (alphabetical: app_main first)
        m = re.search(r'^(#include <nros/[^>]+>)', text, re.MULTILINE)
        if m:
            text = text[:m.start()] + '#include <nros/app_main.h>\n' + text[m.start():]
        else:
            # Fallback — try nros/nros.hpp (C++)
            m = re.search(r'^(#include <nros/nros\.hpp>)', text, re.MULTILINE)
            if m:
                text = text[:m.start()] + '#include <nros/app_main.h>\n' + text[m.start():]

    # 2. Function signature rewrite
    if shim == 'VOID':
        # Match: `extern "C" void app_main(void) {`  OR  `void app_main(void) {`
        text, n = re.subn(
            r'(extern\s+"C"\s+)?void\s+app_main\s*\(\s*void\s*\)\s*\{',
            r'int nros_app_main(int argc, char **argv) {\n    (void)argc;\n    (void)argv;\n',
            text, count=1)
        if n == 0:
            # Already int main? Skip.
            return False
    elif shim == 'ZEPHYR':
        text, n = re.subn(
            r'int\s+main\s*\(\s*void\s*\)\s*\{',
            r'int nros_app_main(int argc, char **argv) {\n    (void)argc;\n    (void)argv;\n',
            text, count=1)
        if n == 0:
            return False
    elif shim == 'POSIX':
        # Native: signature already matches.
        text, n = re.subn(
            r'int\s+main\s*\(\s*int\s+argc\s*,\s*char\s*\*\s*\*\s*argv\s*\)\s*\{',
            r'int nros_app_main(int argc, char **argv) {',
            text, count=1)
        if n == 0:
            return False

    # 3. NROS_CHECK → NROS_CHECK_RET / NROS_TRY_RET (C / C++)
    # The function now returns int instead of void, so existing NROS_CHECK
    # (which does `return;`) won't compile.
    if is_cpp:
        text = re.sub(r'\bNROS_CHECK\((?!_RET)([^;]+?)\);',
                      r'NROS_TRY_RET(\1, 1);', text)
    else:
        text = re.sub(r'\bNROS_CHECK\((?!_RET)([^;]+?)\);',
                      r'NROS_CHECK_RET(\1, 1);', text)

    # 4. Append NROS_APP_MAIN_REGISTER macro at end of file
    macro = f'NROS_APP_MAIN_REGISTER_{shim}()'
    if macro not in text:
        # Strip trailing whitespace, add a blank line then the macro.
        text = text.rstrip() + '\n\n' + macro + '\n'

    path.write_text(text)
    return True


def main():
    if len(sys.argv) != 3:
        print("usage: migrate-app-main.py <shim VOID|ZEPHYR|POSIX> <file>", file=sys.stderr)
        return 1
    shim, file = sys.argv[1], Path(sys.argv[2])
    if migrate(file, shim):
        print(f"migrated: {file}")
    else:
        print(f"skipped (already migrated or pattern not matched): {file}")
    return 0


if __name__ == '__main__':
    sys.exit(main())
