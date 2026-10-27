# Shared RegExp matcher backtracking

The Wasm GC matcher shares one recovery body inside its existing function.
Mismatch sites request recovery through a typed lexical label; they do not copy
choice restoration or encode caller-dependent branch depths. This changes emitted
code sharing, not RegExp search order or the workspace representation.

The recovery wrapper encloses the original candidate-failure block and instruction
dispatch loop without inserting frames inside them. Candidate cursor setup,
choice reset, repeat reset and unmatched capture initialization remain outside
the recovery retry loop. A successful restored continuation retries instruction
dispatch with its restored program counter and cursor, without resetting those
states. Original dispatch fallthrough explicitly leaves the attempt and skips
recovery. An exhausted choice stack reaches the same candidate-failure handling.

The one choice owner retains the original behavior:

- Required-run restoration that selects no continuation keeps searching the
  choice stack. A selected continuation resumes exactly once.
- A failed lazy-progress attempt is discarded and recovery continues. A lazy
  progress choice activates its attempt before restoration.
- Ordinary fallback restores its program counter, byte/UTF-16/low-surrogate
  cursor and complete capture/repeat snapshot before dispatch resumes.

Counted End's optional-empty mismatch uses the same recovery request as the
other opcode failures. Unknown/corrupt state, resource failure, sticky behavior,
candidate advancement and final match/no-match publication retain their existing
routes. No JavaScript callback, new workspace object, host cache or alternate
RegExp execution model is introduced.

The workspace retains exclusive ownership of its demand-grown numeric scratch
tail and checkpoint. Sharing lexical code does not transfer its lease, publish
scratch addresses, change saved snapshot extents or replace the existing typed
choice-entry owners. Existing function-local GC roots remain in the matcher.

Prepared controls should verify one emitted restoration owner, all mismatch
request paths, unchanged local continuation branches, complete capture rollback,
required/lazy/empty counted iterations, nested assertions, Unicode cursor state,
resource failures and scratch retirement. Compilation, semantic verification
and matched measurements for this batch remain pending. Static code sharing does
not establish resident-memory, peak-memory or timing savings.
