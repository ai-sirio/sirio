#!/usr/bin/env python3
"""Find test functions that contain no assertion — tests that cannot fail.

Why this exists
---------------
`probe_escape_dispatch` (rust/crates/sirio/src/main.rs) is a `#[gpui::test]`
with zero assertions and four `eprintln!`s, two of which *compute* the very
condition that should have been asserted (`cx.debug_bounds(..).is_some()`) and
print it instead. It is green, it has always been green, and it can never be
red. It is counted among the workspace's passing tests.

That is this project's most-produced defect class wearing a test's clothing:
something that executes, reports success, and proves nothing. A census is the
only way to find the rest, because every one of them looks like a pass.

Honesty about what a "zero-assert" test still proves
----------------------------------------------------
A test with no `assert*` but with `unwrap()`/`expect()` still proves the code
did not panic. That is weak, but it is not nothing, so those are reported in a
separate tier rather than lumped in. A test with neither proves only that the
code ran to completion.

`eprintln!` inside a zero-assert test is called out separately: it is the
signature of a debugging scaffold that was never converted into a check, and
it means the observation was made and then discarded.

Positive controls
-----------------
This script refuses to report a single absence until it has proved its own
parser works, in both directions:
  * a test known to contain assertions must NOT be flagged, and
  * a test known to contain none (`probe_escape_dispatch`) MUST be flagged.
A negative grep is not evidence of absence until the pattern is proven to
match something. If either control fails the script exits non-zero and prints
nothing else, because a broken detector that prints a clean bill is worse than
no detector.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "rust" / "crates"

TEST_ATTR = re.compile(r"^\s*#\[(?:gpui::test|test|tokio::test)")
FN_LINE = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)")
# The assertion vocabulary must include the codebase's OWN helpers, not just
# the std macros. `dark_palette_matches_waku` asserts entirely through
# `expect_color(..)`, and a std-macros-only pattern reports it as proving
# nothing — a false positive that reads exactly like a finding.
#
# This is worth stating plainly because the positive controls did not catch
# it: 736 tests did contain a std assertion, so the controls passed. A control
# proves the detector is not dead. It bounds false negatives, never false
# positives. Widening the vocabulary is the only fix.
ASSERT = re.compile(
    r"\bassert(?:_eq|_ne|_matches)?!"
    r"|\bpanic!|\bunreachable!"
    r"|\b(?:assert|expect)_[a-z0-9_]+\s*\("  # project helpers: expect_color(..)
    r"|\bpump_until\s*\("  # wait-or-panic: the wait IS the assertion
)

# Three distinct false-positive classes were found by reading the code, each
# only after the tool had already reported the test as proving nothing:
#   1. project assertion helpers   — `expect_color(..)` in sirio_theme
#   2. documented no-op entry points — `helper_process` (subprocess re-exec)
#   3. wait-until-or-panic helpers  — `pump_until(..)`, which ends in
#      `panic!("condition never became true within the pump budget")`, so a
#      test whose only statement is a pump DOES fail when the behaviour is
#      absent.
# Class 3 is the subtle one: `changes_refresh_automatically_after_an_external_
# edit` and `arrow_keys_select_and_space_expands_the_files_tree` look
# assertion-free and are in fact strict. Reporting them would have been the
# expensive kind of wrong — accusing two honest tests, in a file whose owner
# had just cited them as evidence.
PANICKY = re.compile(r"\.unwrap\(\)|\.expect\(")
EPRINTLN = re.compile(r"\beprintln!")


def function_span(lines, start):
    """Return (name, body_lines) for the fn beginning at or after `start`."""
    i = start
    while i < len(lines) and not FN_LINE.match(lines[i]):
        # Skip further attributes / doc comments between the attr and the fn.
        if lines[i].strip() and not lines[i].lstrip().startswith(("#[", "//", "///")):
            return None
        i += 1
    if i >= len(lines):
        return None
    name = FN_LINE.match(lines[i]).group(1)

    # Brace-match from the first `{` at or after the signature.
    depth = 0
    seen = False
    body = []
    while i < len(lines):
        line = lines[i]
        body.append(line)
        for ch in line:
            if ch == "{":
                depth += 1
                seen = True
            elif ch == "}":
                depth -= 1
        if seen and depth <= 0:
            return name, body
        i += 1
    return None


def collect(path):
    lines = path.read_text(errors="replace").splitlines()
    out = []
    for idx, line in enumerate(lines):
        if not TEST_ATTR.match(line):
            continue
        span = function_span(lines, idx + 1)
        if not span:
            continue
        name, body = span
        text = "\n".join(body)
        out.append(
            {
                "file": str(path.relative_to(ROOT.parent.parent)),
                "line": idx + 1,
                "name": name,
                "asserts": len(ASSERT.findall(text)),
                "panicky": bool(PANICKY.search(text)),
                "eprintln": len(EPRINTLN.findall(text)),
            }
        )
    return out


def main():
    tests = []
    for path in sorted(ROOT.rglob("*.rs")):
        tests.extend(collect(path))

    if not tests:
        print("CONTROL FAILED: parsed zero tests — the detector is broken", file=sys.stderr)
        return 1

    by_name = {t["name"]: t for t in tests}

    # Control 1: the detector must find a test known to contain assertions,
    # and must not flag it.
    withassert = [t for t in tests if t["asserts"] > 0]
    if not withassert:
        print(
            "CONTROL FAILED: no test anywhere was seen to contain an assertion; "
            "the assertion pattern does not match, so every 'no assertion' "
            "result below would be an artefact",
            file=sys.stderr,
        )
        return 1

    # Control 2 (false-positive direction, and the one that was missing on the
    # first run): a test that asserts ONLY through a project helper must not be
    # flagged. `dark_palette_matches_waku` asserts via `expect_color(..)` and
    # was reported as proving nothing until the vocabulary was widened.
    # Without this control the script's own output is unfalsifiable.
    helper_asserting = "dark_palette_matches_waku"
    if helper_asserting in by_name and by_name[helper_asserting]["asserts"] == 0:
        print(
            f"CONTROL FAILED: {helper_asserting} asserts through a project "
            "helper (expect_color) but was counted as assertion-free; the "
            "assertion vocabulary is too narrow and every result below would "
            "over-report",
            file=sys.stderr,
        )
        return 1

    # Tests that are no-ops on purpose, with the reason they are exempt. These
    # are excluded by name and the exclusion is printed, never silent.
    BY_DESIGN = {
        "helper_process": "documented subprocess entry point; a no-op unless "
                          "TILLER_PERSISTENCE_HELPER is set",
        "writer_process": "documented subprocess entry point; same contract",
    }

    # Control 3: the known assertion-free test must be flagged.
    known = "probe_escape_dispatch"
    if known in by_name:
        if by_name[known]["asserts"] != 0:
            print(
                f"CONTROL FAILED: {known} is known to contain no assertion, "
                f"but the detector counted {by_name[known]['asserts']}",
                file=sys.stderr,
            )
            return 1
    else:
        print(
            f"NOTE: the control test `{known}` is no longer in the tree. It may "
            "have been fixed or removed; re-establish a control before trusting "
            "this run.",
            file=sys.stderr,
        )

    bare = [t for t in tests if t["asserts"] == 0]
    exempt = [t for t in bare if t["name"] in BY_DESIGN]
    bare = [t for t in bare if t["name"] not in BY_DESIGN]
    for t in exempt:
        print(f"excluded by design: {t['name']} — {BY_DESIGN[t['name']]}")
    if exempt:
        print()
    proves_nothing = [t for t in bare if not t["panicky"]]
    non_panic_only = [t for t in bare if t["panicky"]]

    print(f"controls passed: {len(tests)} tests parsed, "
          f"{len(withassert)} contain an assertion")
    print()
    print(f"{len(bare)} of {len(tests)} tests contain no assertion "
          f"({100 * len(bare) // max(len(tests), 1)}%)")
    print(f"  {len(proves_nothing):4} prove nothing at all "
          f"(no assert, no unwrap/expect)")
    print(f"  {len(non_panic_only):4} prove only that nothing panicked")
    print()

    discarded = [t for t in bare if t["eprintln"]]
    if discarded:
        print("Tests that PRINT an observation instead of asserting it "
              "(a scaffold never converted into a check):")
        for t in sorted(discarded, key=lambda x: -x["eprintln"]):
            print(f"  {t['file']}:{t['line']}  {t['name']}  "
                  f"({t['eprintln']} eprintln!)")
        print()

    if proves_nothing:
        print("No assertion and no panic path — green by construction:")
        for t in proves_nothing:
            print(f"  {t['file']}:{t['line']}  {t['name']}")
        print()

    if non_panic_only:
        print(f"No assertion, but unwrap/expect can panic "
              f"({len(non_panic_only)} — weaker, not empty):")
        for t in non_panic_only[:40]:
            print(f"  {t['file']}:{t['line']}  {t['name']}")
        if len(non_panic_only) > 40:
            print(f"  ... and {len(non_panic_only) - 40} more")

    return 0


if __name__ == "__main__":
    sys.exit(main())
